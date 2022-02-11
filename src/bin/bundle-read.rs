/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use anyhow::Result;
use atari_bundle::BundleConfig;
use structopt::StructOpt;
use zip::ZipArchive;

#[derive(Debug, StructOpt)]
#[structopt(name = "bundle-read", long_about = "Query bundle metadata.")]
struct Opt {
    #[structopt(name = "BUNDLE", help = "The bundle file to query.")]
    bundle: String,
    #[structopt(long, short, help = "The field to output, one value line for lists.")]
    field: Option<String>,
    #[structopt(
        long,
        short,
        help = "Output everything, with multiple value fields comma separated."
    )]
    all: bool,
}

fn read_bundle<P: AsRef<Path>>(path: P) -> Result<BundleConfig> {
    let file = File::open(path.as_ref())?;
    let mut za = ZipArchive::new(file)?;
    Ok(BundleConfig::from_archive(&mut za)?)
}

trait ToClonedVec<T> {
    fn to_cloned_vec(&self) -> Option<Vec<T>>;
}

impl<T> ToClonedVec<T> for Option<T>
where
    T: Clone,
{
    fn to_cloned_vec(&self) -> Option<Vec<T>> {
        self.as_ref().map(|x| vec![x.clone()])
    }
}

impl ToClonedVec<String> for bool {
    fn to_cloned_vec(&self) -> Option<Vec<String>> {
        self.then(|| vec!["true".to_string()])
    }
}

fn bundle_to_map(bundle: &BundleConfig) -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        ("Name", Some(vec![bundle.bundle.name.to_string()])),
        ("Type", Some(vec![bundle.bundle.bundle_type.to_string()])),
        ("StoreID", bundle.bundle.store_id.to_cloned_vec()),
        ("HomebrewID", bundle.bundle.homebrew_id.to_cloned_vec()),
        ("Exec", bundle.bundle.exec.to_cloned_vec()),
        (
            "EncryptedImage",
            bundle.bundle.encrypted_image.to_cloned_vec(),
        ),
        ("Version", bundle.bundle.version.to_cloned_vec()),
        ("Background", bundle.bundle.background.to_cloned_vec()),
        (
            "PreferXBoxMode",
            bundle.bundle.prefer_xbox_mode.to_cloned_vec(),
        ),
        ("Launcher", bundle.bundle.launcher.to_cloned_vec()),
        ("LauncherTags", {
            if bundle.bundle.launcher_tags.is_empty() {
                None
            } else {
                Some(bundle.bundle.launcher_tags.clone())
            }
        }),
        ("LauncherExec", bundle.bundle.launcher_exec.to_cloned_vec()),
    ])
    .into_iter()
    .filter_map(|kv| {
        let (k, v) = kv;
        v.map(|x| (k.to_string(), x))
    })
    .collect()
}

fn main() -> Result<()> {
    env_logger::init();

    let opt = Opt::from_args();

    let bundle = read_bundle(opt.bundle)?;

    let fields = bundle_to_map(&bundle);
    if opt.all {
        for (field, slice) in fields.iter() {
            println!("{}:", field);
            for item in slice {
                println!("  {}", item);
            }
        }
    } else if let Some(field) = opt.field {
        if let Some(slice) = fields.get(&field) {
            for item in slice {
                println!("{}", item);
            }
        } else {
            std::process::exit(1);
        }
    }

    Ok(())
}
