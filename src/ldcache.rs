/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use log::{debug, error, trace};
use thiserror::Error;

use crate::FileEntry;

#[derive(Debug, Error)]
pub enum LdError {
    #[error("IO error occurred: {0}")]
    Io(#[from] std::io::Error),
    #[error("Elf error occurred: {0}")]
    Elf(#[from] goblin::error::Error),
    #[error("Cache error: {0}")]
    Cache(#[from] ldcache_rs::CacheError),
    #[error("Not an ELF-format archive")]
    NotElf,
    #[error("Missing dependency: {0}")]
    MissingDependency(String),
}

fn find_elf_deps<P: AsRef<Path>>(item: P) -> Result<Vec<String>, LdError> {
    let buf = fs::read(item)?;
    let e = goblin::Object::parse(&buf)?;
    let e = if let goblin::Object::Elf(e) = e {
        Ok(e)
    } else {
        Err(LdError::NotElf)
    }?;

    Ok(if let Some(d) = e.dynamic {
        d.get_libraries(&e.dynstrtab)
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    })
}

fn load_base_deps<P: AsRef<Path>>(f: P) -> Result<ldcache_rs::Cache, LdError> {
    let buf = fs::read(f.as_ref()).map_err(|e| {
        error!("Unable to open ldcache with fs::read: {:?}", e);
        e
    })?;
    Ok(ldcache_rs::Cache::parse(
        &buf,
        ldcache_rs::TargetEndian::Native,
    )?)
}

fn find_additional_versions(
    elf: &FileEntry,
    libs_out: &mut BTreeMap<OsString, FileEntry>,
) -> Result<(), LdError> {
    trace!(
        "Insert all linked versions of {}",
        elf.location.to_string_lossy()
    );
    if let Some(file) = elf.location.file_name() {
        if let Some(parent) = elf.location.parent() {
            let chunks = file
                .to_string_lossy()
                .split('.')
                .map(str::to_string)
                .collect::<Vec<_>>();
            for i in 1..=chunks.len() {
                let dep = OsString::from(&chunks[0..i].join("."));
                let candidate = parent.join(&dep);
                trace!("  - processing candidate {}", candidate.to_string_lossy());
                match fs::canonicalize(candidate) {
                    Ok(canon) => {
                        trace!("    - can be canoncalized to {}", canon.to_string_lossy());
                        if canon == elf.location {
                            trace!("    - inserted symlink {}", dep.to_string_lossy());
                            libs_out.insert(
                                dep.clone(),
                                FileEntry {
                                    name: if let Some(name_parent) = Path::new(&elf.name).parent() {
                                        name_parent.join(dep).to_string_lossy().to_string()
                                    } else {
                                        dep.to_string_lossy().to_string()
                                    },
                                    location: elf.location.clone(),
                                },
                            );
                        } else {
                            trace!(
                                "    - points to different location from {}",
                                elf.location.to_string_lossy()
                            );
                        }
                    }
                    Err(e) => {
                        trace!("    - can't be canonicalized: {}", e);
                    }
                };
            }
        }
    }
    Ok(())
}

pub fn resolve_deps(elves: Vec<FileEntry>) -> Result<Vec<FileEntry>, LdError> {
    let base_deps = load_base_deps("/usr/local/share/bundle-gen/ld.so.cache.vcs").map_err(|e| {
        error!("Unable to load ldcache: {:?}", e);
        e
    })?;
    let build_deps = ldcache_rs::Cache::new()?;
    let mut work = Vec::<PathBuf>::new();
    let mut queued = BTreeSet::new();
    let mut own_libs = BTreeSet::new();
    let mut own_extra_libs = BTreeMap::new();
    let mut res = Vec::new();

    for elf in elves {
        let pb = elf.location.clone();
        if !queued.contains(&pb) {
            if let Some(file) = Path::new(&elf.name).file_name() {
                own_libs.insert(file.to_os_string());
            }
            find_additional_versions(&elf, &mut own_extra_libs)?;
            queued.insert(pb.clone());
            work.push(pb);
        }
    }

    while let Some(item) = work.pop() {
        trace!("Processing {} for dependencies", item.to_string_lossy());
        match find_elf_deps(&item) {
            Ok(deps) => {
                for d in deps {
                    trace!(" - {}", d);
                    if !base_deps.contains(&d) && !own_libs.contains(&OsString::from(&d)) {
                        if let Some(entry) = own_extra_libs.get(&OsString::from(&d)) {
                            res.push(entry.clone());
                            own_libs.insert(OsString::from(d));
                        } else {
                            match build_deps.get_path(&d) {
                                Some(p) => {
                                    let p: &Path = p.as_ref();
                                    let p = p.to_path_buf();
                                    if !queued.contains(&p) {
                                        queued.insert(p.clone());
                                        res.push(FileEntry {
                                            name: Path::new("lib")
                                                .join(&d)
                                                .to_string_lossy()
                                                .to_string(),
                                            location: p.clone(),
                                        });
                                        work.push(p);
                                    }
                                }
                                None => {
                                    debug!("Missing dependency: {:?}", d);
                                    debug!("Own libs are:");
                                    for dep in own_libs {
                                        debug!(" - {}", dep.to_string_lossy());
                                    }
                                    return Err(LdError::MissingDependency(d));
                                }
                            }
                        }
                    }
                }
            }
            Err(LdError::NotElf) => {
                /* Ignore non-elf files, to make things a little simpler for the user */
                trace!("Non-ELF file {} ignored", item.to_string_lossy());
            }
            Err(e) => return Err(e),
        }
    }

    Ok(res)
}
