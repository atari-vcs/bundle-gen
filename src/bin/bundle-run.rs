/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{anyhow, Result};
use atari_bundle::BundleConfig;
use log::{error, trace};
use structopt::StructOpt;
use zip::read::ZipArchive;

trait XdgDataDirsExt {
    fn add_xdg_dirs(&mut self) -> &mut Self;
}

impl XdgDataDirsExt for Command {
    fn add_xdg_dirs(&mut self) -> &mut Self {
        if let Some(home) = self
            .get_envs()
            .filter(|(k, _)| k == &OsStr::new("HOME"))
            .map(|(_, v)| v.map(PathBuf::from))
            .flatten()
            .next()
        {
            trace!("Adding XDG runtime directory for a home of {:?}", home);
            self.env("XDG_RUNTIME_DIR", home.join(".runtime"));
        }
        self
    }
}

fn data_dir<R: Read + Seek>(za: &mut ZipArchive<R>) -> Result<(String, PathBuf)> {
    let base = Path::new("/home/games");
    let config = BundleConfig::from_archive(za)?;
    if let Some(store_id) = config.bundle.store_id {
        Ok((
            store_id.clone(),
            base.join("bundle-data").join(Path::new(&store_id)),
        ))
    } else if let Some(homebrew_id) = config.bundle.homebrew_id {
        let h = format!("homebrew-{}", homebrew_id);
        Ok((h.clone(), base.join("bundle-data").join(PathBuf::from(&h))))
    } else {
        Err(anyhow!("Corrupt bundle has no ID"))
    }
}

fn extract_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<(String, PathBuf)> {
    trace!("Extracting bundle {:?}", bundle_path.as_ref());
    let file = File::open(bundle_path.as_ref())?;
    let mut za = ZipArchive::new(file)?;
    let (id, data) = data_dir(&mut za)?;
    trace!("Found bundle directory of {} at {:?}", id, data);
    fs::create_dir_all(&data)?;
    za.extract(&data)?;
    trace!("Bundle extracted successfully.");
    patch_bundle(&data)?;
    Ok((id, data))
}

fn patch_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<()> {
    let patch_file = bundle_path.as_ref().join("runner-patch");
    if patch_file.exists() {
        trace!("Patching bundle for runner");
        run_command(Command::new(patch_file).current_dir(bundle_path.as_ref()))?;
    }
    Ok(())
}

fn make_home_dir<P: AsRef<Path>>(bundle_id: P) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home directory set"))?;
    let path = home.join("bundle-home").join(bundle_id);
    trace!("Making home directory {:?}", path);
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn run_command(cmd: &mut Command) -> Result<ExitStatus> {
    let output = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;

    let stdout = std::str::from_utf8(&output.stdout)?;
    let stderr = std::str::from_utf8(&output.stderr)?;

    println!("{}", stdout);
    eprintln!("{}", stderr);

    Ok(output.status)
}

fn switch_user(login: &str) -> Result<()> {
    let user = users::get_user_by_name(login)
        .ok_or_else(|| anyhow!("cannot find requested user {}", login))?;
    users::switch::set_both_gid(user.primary_group_id(), user.primary_group_id()).map_err(|e| {
        error!(
            "Switching to GID {0}/{0} failed: {1}",
            user.primary_group_id(),
            e
        );
        e
    })?;
    users::switch::set_both_uid(user.uid(), user.uid()).map_err(|e| {
        error!("Switching to UID {0}/{0} failed: {1}", user.uid(), e);
        e
    })?;
    std::env::remove_var("HOME");

    Ok(())
}

fn run_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<()> {
    let (id, data_dir) = extract_bundle(bundle_path)?;

    switch_user("user")?;

    let home = make_home_dir(id)?;

    trace!("Loading bundle.ini");
    let spec = BundleConfig::from_read(File::open(data_dir.join("bundle.ini"))?)?;

    if let Some(exec) = spec.bundle.exec {
        run_command(
            Command::new(data_dir.join(exec))
                .current_dir(&data_dir)
                .env("HOME", &home)
                .add_xdg_dirs(),
        )?;
    }
    Ok(())
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(name = "FILE")]
    bundle: String,
}

fn main() -> Result<()> {
    env_logger::init();

    let opt = Opt::from_args();

    run_bundle(opt.bundle)
}
