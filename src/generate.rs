/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Seek, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use atari_bundle::{BundleConfig, BundleError};
use log::{trace, warn};
use thiserror::Error;
use zip::ZipWriter;

use crate::config::{BuildSpec, BundleSpec, BundleSpecError};
use crate::ldcache::{self, LdError};
use crate::FileEntry;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("could not find file {}", .0.to_string_lossy())]
    Find(PathBuf),
    #[error("IO error while reading {0}: {1}")]
    IO(PathBuf, std::io::Error),
    #[error("IO error while reading entry {0}: {1}")]
    ZipIO(String, std::io::Error),
    #[error("IO error finding current directory: {0}")]
    EnvIO(std::io::Error),
    #[error("unable to write to bundle: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("a file was expected for {}", .0.to_string_lossy())]
    ExpectedFile(PathBuf),
    #[error("cannot find parent directory of {}", .0.to_string_lossy())]
    NoParent(PathBuf),
    #[error("unable to process link dependencies: {0}")]
    Ld(#[from] LdError),
    #[error("error processing bundle configuration: {0}")]
    Bundle(#[from] BundleError),
    #[error("error reading bundle specification: {0}")]
    BundleSpec(#[from] BundleSpecError),
    #[error("build stage failed")]
    Build,
    #[error("bundle must be either a store or homebrew bundle")]
    BundleOriginUnknown,
    #[error("Utf-8 error occurred while processing {0}: {1}")]
    Utf8(String, std::str::Utf8Error),
    #[error("the field {0} is not permitted in this context")]
    InvalidField(String),
    #[error("the field {0} must be present based on the other values given")]
    MissingField(String),
    #[error("bad command {0} didn't contain a program after parsing")]
    BadCommand(String),
    #[error("the bundle entry {0} was specified multiple times")]
    DuplicateZipFileEntry(String),
}

type BuildResult<T> = Result<T, BuildError>;

struct PathContext {
    locations: Vec<PathBuf>,
}

impl PathContext {
    pub fn new(locations: Vec<PathBuf>) -> Self {
        Self { locations }
    }

    fn find_path<P: AsRef<Path>>(&self, target: P) -> BuildResult<PathBuf>
    where
        P: AsRef<Path>,
    {
        for loc in &self.locations {
            let p = loc.join(target.as_ref());
            if p.exists() {
                return Ok(p);
            }
        }
        Err(BuildError::Find(target.as_ref().to_path_buf()))
    }
}

fn process_dir<P, Q>(path: P, entry_name: Q, files: &mut Vec<FileEntry>) -> BuildResult<()>
where
    P: AsRef<Path>,
    Q: Into<String>,
{
    let e = entry_name.into();
    trace!("processing dir {:?} under entry {}", path.as_ref(), e);
    for entry in path
        .as_ref()
        .read_dir()
        .map_err(|e| BuildError::IO(path.as_ref().to_path_buf(), e))?
    {
        match entry {
            Ok(entry) => {
                if let Some(relpath) = pathdiff::diff_paths(entry.path(), &path) {
                    let kind = entry
                        .metadata()
                        .map_err(|e| BuildError::IO(entry.path(), e))?
                        .file_type();
                    if kind.is_file() {
                        files.push(FileEntry {
                            location: entry.path(),
                            name: Path::new(&e).join(relpath).to_string_lossy().to_string(),
                        });
                    } else if kind.is_dir() {
                        process_dir(
                            entry.path(),
                            Path::new(&e).join(relpath).to_string_lossy().to_string(),
                            files,
                        )?;
                    } else {
                        warn!("skipped entry: only files and directories are supported");
                    }
                } else {
                    warn!("skipped entry: not a valid path");
                }
            }
            Err(e) => {
                warn!("skipped entry: {}", e);
            }
        }
    }
    Ok(())
}

fn insert_files<W>(zf: &mut zip::ZipWriter<W>, files: &[FileEntry]) -> BuildResult<()>
where
    W: Write + Seek,
{
    let mut entry_map = BTreeMap::new();

    for file in files {
        if let Some(old_location) = entry_map.insert(file.name.clone(), file.location.clone()) {
            if old_location != file.location {
                return Err(BuildError::DuplicateZipFileEntry(file.name.clone()));
            }
        }
    }

    let mut last_path: Option<String> = None;
    for kv in entry_map {
        let (name, location) = &kv;
        let mut old_comps = match last_path {
            Some(path) => Path::new(&path)
                .ancestors()
                .map(|p| p.to_path_buf())
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        let mut new_comps = Path::new(&name).ancestors().collect::<Vec<_>>();
        old_comps.reverse();
        new_comps.reverse();
        // discard the actual files
        old_comps.pop();
        new_comps.pop();
        // now we have the list of directories each file is contained in;
        // we start at 1 to avoid adding a blank root directory
        for i in 1..new_comps.len() {
            if i >= old_comps.len() || old_comps[i] != new_comps[i] {
                trace!("insert directory {}", new_comps[i].to_string_lossy());
                zf.add_directory(
                    new_comps[i].to_string_lossy(),
                    zip::write::FileOptions::default(),
                )?;
            }
        }

        trace!("insert file {}", name);

        let meta = std::fs::metadata(location).map_err(|e| BuildError::IO(location.clone(), e))?;
        let options = zip::write::FileOptions::default()
            .large_file(meta.len() >= (1u64 << 32))
            .unix_permissions(meta.permissions().mode());

        zf.start_file(name, options)?;
        let mut subfile = File::open(&location).map_err(|e| BuildError::IO(location.clone(), e))?;
        std::io::copy(&mut subfile, zf).map_err(|e| BuildError::IO(location.clone(), e))?;

        last_path = Some(name.clone());
    }
    Ok(())
}

fn parse_version_file<P: Clone + AsRef<Path>>(path: P) -> BuildResult<String> {
    Ok(fs::read_to_string(path.clone())
        .map_err(|e| BuildError::IO(path.as_ref().to_path_buf(), e))?
        .trim()
        .to_string())
}

fn run_command<W: Write>(cmd: &mut Command, mut log: W) -> BuildResult<()> {
    let prog = cmd.get_program().to_os_string();
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;

    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|e| BuildError::Utf8(prog.to_string_lossy().to_string(), e))?;
    let stderr = std::str::from_utf8(&output.stderr)
        .map_err(|e| BuildError::Utf8(prog.to_string_lossy().to_string(), e))?;

    writeln!(log, "STDOUT:").map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;
    writeln!(log, "{}", stdout).map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;
    writeln!(log).map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;
    writeln!(log, "STDERR:").map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;
    writeln!(log, "{}", stderr).map_err(|e| BuildError::IO(Path::new(&prog).to_path_buf(), e))?;

    if !output.status.success() {
        println!("{}", stdout);
        eprintln!("{}", stderr);
        return Err(BuildError::Build);
    }
    Ok(())
}

fn process_file_items<P, Q>(
    items: &[P],
    base_path: Q,
    pc: &PathContext,
    entries: &mut Vec<FileEntry>,
) -> BuildResult<()>
where
    P: AsRef<Path>,
    Q: Into<String>,
{
    let s = base_path.into();
    for item in items {
        let path = fs::canonicalize(pc.find_path(item)?)
            .map_err(|e| BuildError::IO(item.as_ref().to_path_buf(), e))?;
        let meta = fs::metadata(&path).map_err(|e| BuildError::IO(path.clone(), e))?;

        if let Some(filename) = item.as_ref().file_name() {
            if meta.is_file() {
                entries.push(FileEntry {
                    location: path,
                    name: Path::new(&s).join(filename).to_string_lossy().to_string(),
                });
            } else if meta.is_dir() {
                let zip_path = Path::new(&s);
                let filename = if !item.as_ref().to_string_lossy().ends_with('/') {
                    zip_path.join(filename)
                } else {
                    zip_path.to_path_buf()
                };
                process_dir(path, filename.to_string_lossy().to_string(), entries)?;
            } else {
                warn!("skipped entry: only files and directories are supported");
            }
        } else {
            warn!("skipped entry: not a valid path");
        }
    }

    Ok(())
}

fn build_phase(
    b: &BuildSpec,
    stem: &str,
    pc: &PathContext,
) -> BuildResult<(PathBuf, ZipWriter<File>, String)> {
    let mut log_file = PathBuf::from(stem);
    log_file.set_extension("log");
    let mut build_log = File::create(&log_file).map_err(|e| BuildError::IO(log_file, e))?;

    if let Some(ref deps) = b.required_packages {
        run_command(
            Command::new("apt-get")
                .arg("install")
                .arg("-y")
                .env("DEBIAN_FRONTEND", "noninteractive")
                .args(deps),
            &mut build_log,
        )?;
    }

    if let Some(ref modules) = b.required_modules {
        for module in modules {
            // Install/build the module
            let path = pc.find_path(module)?;
            trace!("Discovered module file at {:?}", path);
            run_command(&mut Command::new(&path), &mut build_log)?;
        }

        run_command(&mut Command::new("ldconfig"), &mut build_log)?;
    }

    // Do the build itself
    if let Some(ref cmd) = b.build_command {
        let path = pc.find_path(cmd)?;
        run_command(&mut Command::new(&path), &mut build_log)?;
    }

    let mut executables_on_disk = Vec::new();
    if let Some(ref executables) = b.executables {
        process_file_items(executables, "bin", pc, &mut executables_on_disk)?;
    }

    let mut libraries_on_disk = Vec::new();
    if let Some(ref libraries) = b.libraries {
        process_file_items(libraries, "lib", pc, &mut libraries_on_disk)?;
    }

    let mut resources_on_disk = Vec::new();
    if let Some(ref resources) = b.resources {
        process_file_items(resources, "res", pc, &mut resources_on_disk)?;
    }

    // These are elf files that we believe hold dependencies we'd otherwise miss,
    // but don't get installed into the bundle by listing them here (they can
    // still be installed by listing them under resources, for example).
    let mut extra_elf_on_disk = Vec::new();
    if let Some(ref files) = b.extra_elf_files {
        process_file_items(files, "_unused", pc, &mut extra_elf_on_disk)?;
    }

    // elf files that can't provide dependencies, like executables and plugins
    let elves = executables_on_disk
        .iter()
        .chain(extra_elf_on_disk.iter())
        .chain(libraries_on_disk.iter())
        .cloned()
        .collect::<Vec<_>>();

    trace!("Have ELF files on disk (initial) as:");
    for elf in elves.iter() {
        trace!(" - {}", elf.location.to_string_lossy());
    }

    let dependencies_on_disk = ldcache::resolve_deps(elves)?;

    let version = parse_version_file(pc.find_path(&b.version_file)?)?;
    let output = format!("{}_{}.bundle", stem, version);
    let f = File::create(output.clone())
        .map_err(|e| BuildError::IO(Path::new(&output).to_path_buf(), e))?;
    let mut zf = zip::ZipWriter::new(f);
    insert_files(
        &mut zf,
        &executables_on_disk
            .into_iter()
            .chain(libraries_on_disk.into_iter())
            .chain(resources_on_disk.into_iter())
            .chain(dependencies_on_disk.into_iter())
            .collect::<Vec<_>>(),
    )?;

    Ok((PathBuf::from(output), zf, version))
}

fn make_launcher_sh<W: Write + Seek>(
    zf: &mut ZipWriter<W>,
    name: &str,
    startup_command: &str,
) -> BuildResult<()> {
    let options = zip::write::FileOptions::default().unix_permissions(0o755);
    zf.start_file(name, options)?;

    let (cmd, args) = match shell_words::split(startup_command) {
        Ok(parts) => {
            let mut iter = parts.iter();
            let cmd = iter
                .next()
                .ok_or_else(|| BuildError::BadCommand(startup_command.to_string()))?;
            let args = iter.map(|s| format!("'{}'", s)).collect::<Vec<_>>();
            (cmd.clone(), args)
        }
        Err(_) => (startup_command.to_string(), Vec::new()),
    };

    writeln!(
        zf,
        r#"#!/bin/sh

set -x

P=$(dirname "$(busybox realpath "$0")")

export LD_LIBRARY_PATH="${{LD_LIBRARY_PATH}}:${{P}}/lib"

"${{P}}/{}" {} "$@"
"#,
        cmd,
        args.join(" ")
    )
    .map_err(|e| BuildError::ZipIO(name.to_string(), e))?;

    Ok(())
}

fn make_bundle(cfg: &BundleSpec, stem: &str, pc: &PathContext) -> BuildResult<PathBuf> {
    let (path, mut zf, version) = build_phase(&cfg.build, stem, pc)?;

    let prog = if let Some(ref exec) = cfg.exec {
        if cfg.launcher.is_some() {
            // If some other program will launch us, then startup command
            // is just our arguments
            Some(exec.clone())
        } else {
            // If not, make a simple script to wrap this program and set
            // up its libraries
            make_launcher_sh(&mut zf, "run.sh", exec)?;
            Some("run.sh".to_string())
        }
    } else {
        None
    };

    let builder = BundleConfig::builder(cfg.name.clone(), cfg.bundle_type);

    if let Some(ref homebrew_id) = cfg.homebrew_id {
        let mut builder = builder.homebrew_id(homebrew_id.clone());
        builder
            .set_exec(prog)
            .set_prefer_xbox_mode(cfg.prefer_xbox_mode)
            .set_version(Some(version))
            .set_requires_launcher(cfg.launcher.clone());
        if cfg.launcher_tags.is_some() {
            return Err(BuildError::InvalidField("LauncherTags".to_string()));
        } else if cfg.launcher_exec.is_some() {
            return Err(BuildError::InvalidField("LauncherExec".to_string()));
        } else if cfg.background.is_some() {
            return Err(BuildError::InvalidField("Background".to_string()));
        }
        builder.build().to_archive(&mut zf)?;
    } else if let Some(ref store_id) = cfg.store_id {
        let mut builder = builder.store_id(store_id.clone());
        builder
            .set_exec(prog)
            .set_prefer_xbox_mode(cfg.prefer_xbox_mode)
            .set_version(Some(version))
            .set_requires_launcher(cfg.launcher.clone())
            .set_background(cfg.background);

        if let Some(ref launcher) = cfg.launcher_exec {
            if let Some(ref tags) = cfg.launcher_tags {
                make_launcher_sh(&mut zf, "launch.sh", launcher)?;
                builder.set_provides_launcher(Some("launch.sh".to_string()), tags.clone());
            } else {
                return Err(BuildError::MissingField("LauncherTags".to_string()));
            }
        } else if cfg.launcher_tags.is_some() {
            return Err(BuildError::MissingField("LauncherExec".to_string()));
        }
        builder.build().to_archive(&mut zf)?;
    } else {
        return Err(BuildError::BundleOriginUnknown);
    }

    if let Some(ref patchfile) = cfg.runner_patch {
        insert_files(
            &mut zf,
            &[FileEntry {
                name: "runner-patch".to_string(),
                location: fs::canonicalize(patchfile)
                    .map_err(|e| BuildError::IO(Path::new(patchfile).to_path_buf(), e))?,
            }],
        )?;
    }

    zf.finish()?;

    Ok(path)
}

pub fn generate<P: AsRef<Path>>(arg: P) -> BuildResult<PathBuf> {
    let wd = std::env::current_dir().map_err(BuildError::EnvIO)?;
    let path = PathBuf::from(&arg.as_ref().as_os_str());
    let spec_dir = fs::canonicalize(&path)
        .map_err(|e| BuildError::IO(arg.as_ref().to_path_buf(), e))
        .and_then(|p| {
            p.parent()
                .map(Path::to_path_buf)
                .ok_or(BuildError::NoParent(p))
        })?;

    let pc = PathContext::new(vec![wd, spec_dir]);

    let spec = BundleSpec::load(&arg)?;

    // Check for basic errors in the spec
    BundleSpec::check(&spec)?;

    let stem = path
        .file_stem()
        .ok_or_else(|| BuildError::ExpectedFile(path.clone()))?
        .to_string_lossy();

    make_bundle(&spec, &stem, &pc)
}
