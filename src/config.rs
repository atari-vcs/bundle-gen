/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::fs::File;
use std::path::Path;

use atari_bundle::BundleType;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundleSpecError {
    #[error("IO error opening spec: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("conflicting origins specified; only one of StoreID or HomebrewID is permitted")]
    ConflictingOrigins,
    #[error("unusable launcher has no associated launcher tags")]
    NoLauncherTags,
    #[error("no LauncherExec present, but one was expected based on the provided {0}")]
    NoLauncherExec(String),
    #[error("no Exec present, but one was expected based on the provided {0}")]
    NoExec(String),
    #[error("Exec present, but will never be used based on BundleType")]
    UselessExec,
    #[error("Homebrew bundles cannot be launchers")]
    NoHomebrewLaunchers,
    #[error("Homebrew bundles cannot be run in the background")]
    NoHomebrewBackgroundBundles,
    #[error("a bundle must have a unique ID")]
    NoOriginId,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct BuildSpec {
    pub version_file: String,
    pub required_packages: Option<Vec<String>>,
    pub build_command: Option<String>,
    pub executables: Option<Vec<String>>,
    pub libraries: Option<Vec<String>>,
    pub resources: Option<Vec<String>>,
    pub extra_elf_files: Option<Vec<String>>,
    pub required_modules: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct BundleSpec {
    pub name: String,
    #[serde(rename = "Type")]
    pub bundle_type: BundleType,
    #[serde(rename = "StoreID")]
    pub store_id: Option<String>,
    #[serde(rename = "HomebrewID")]
    pub homebrew_id: Option<String>,
    pub exec: Option<String>,
    pub background: Option<bool>,
    #[serde(rename = "PreferXBoxMode")]
    pub prefer_xbox_mode: Option<bool>,
    pub launcher: Option<String>,
    #[serde(default)]
    pub launcher_tags: Option<Vec<String>>,
    pub launcher_exec: Option<String>,
    pub runner_patch: Option<String>,
    pub build: BuildSpec,
}

impl BundleSpec {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<BundleSpec, BundleSpecError> {
        let file = File::open(path.as_ref())?;
        Ok(serde_yaml::from_reader(file)?)
    }

    fn check_store_bundle(spec: &BundleSpec) -> Result<(), BundleSpecError> {
        if spec.homebrew_id.is_some() {
            return Err(BundleSpecError::ConflictingOrigins);
        }

        let tags = spec.launcher_tags.clone().unwrap_or_default();
        if tags.is_empty() {
            if spec.launcher_exec.is_some() {
                return Err(BundleSpecError::NoLauncherTags);
            }
        } else if spec.launcher_exec.is_none() {
            return Err(BundleSpecError::NoLauncherExec("launcher tags".to_string()));
        }

        match spec.bundle_type {
            BundleType::Game | BundleType::Application => {
                if spec.exec.is_none() {
                    return Err(BundleSpecError::NoExec("bundle type".to_string()));
                }
            }
            BundleType::LauncherOnly => {
                if spec.launcher_exec.is_none() {
                    return Err(BundleSpecError::NoLauncherExec("bundle type".to_string()));
                }
                if spec.exec.is_some() {
                    return Err(BundleSpecError::UselessExec);
                }
            }
        };

        if spec.launcher_exec.is_some() {
            match &spec.launcher_tags {
                Some(v) if v.is_empty() => {
                    return Err(BundleSpecError::NoLauncherTags);
                }
                None => {
                    return Err(BundleSpecError::NoLauncherTags);
                }
                _ => {}
            };
        };

        Ok(())
    }

    fn check_homebrew_bundle(spec: &BundleSpec) -> Result<(), BundleSpecError> {
        let tags = spec.launcher_tags.clone().unwrap_or_default();
        if !tags.is_empty() || spec.launcher_exec.is_some() {
            return Err(BundleSpecError::NoHomebrewLaunchers);
        }

        match spec.bundle_type {
            BundleType::Game | BundleType::Application => {
                if spec.exec.is_none() {
                    return Err(BundleSpecError::NoExec("bundle type".to_string()));
                }
            }
            BundleType::LauncherOnly => {
                return Err(BundleSpecError::NoHomebrewLaunchers);
            }
        };

        if spec.background.unwrap_or_default() {
            return Err(BundleSpecError::NoHomebrewBackgroundBundles);
        }

        Ok(())
    }

    pub fn check(spec: &BundleSpec) -> Result<(), BundleSpecError> {
        if spec.store_id.is_some() {
            BundleSpec::check_store_bundle(spec)?;
        } else if spec.homebrew_id.is_some() {
            BundleSpec::check_homebrew_bundle(spec)?;
        } else {
            return Err(BundleSpecError::NoOriginId);
        }

        Ok(())
    }
}
