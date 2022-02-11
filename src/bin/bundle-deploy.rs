use anyhow::{anyhow, Result};
use atari_bundle::BundleConfig;
use chrono::{DateTime, Utc};
use reqwest::blocking::multipart::Form;
use reqwest::blocking::Client;
use structopt::StructOpt;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use std::env;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::num::ParseIntError;
use std::path::{Path, PathBuf};

fn parse_version(src: &str) -> Result<u32> {
    src.parse::<u32>()
        .map_err(ParseIntError::into)
        .and_then(|x| {
            if x == 0 {
                Err(anyhow!("Version number 0 is not permitted."))
            } else {
                Ok(x)
            }
        })
}

/// The data that can legitimately be set by the caller (as opposed to
/// inferred)
#[derive(Clone, Debug, StructOpt)]
#[structopt(
    name = "bundle-deploy",
    long_about = "Tool for deploying bundles to the store."
)]
struct Options {
    #[structopt(
        long,
        short = "d",
        help = "",
        long_help = "The release date that should be shown for this bundle."
    )]
    release_date: Option<DateTime<Utc>>,
    #[structopt(
        long,
        short = "n",
        help = "",
        long_help = "The first part of any user-visible release note."
    )]
    release_note: Option<String>,
    #[structopt(
        long,
        help = "",
        long_help = "The second part of any user-visible release note."
    )]
    additional_release_note: Option<String>,
    #[structopt(
        long,
        short = "V",
        help = "The store version.",
        long_help = "The store version of the bundle, where higher is newer, and will be preferred. It should increment monotonically.",
        parse(try_from_str=parse_version),
    )]
    store_version: u32,
    #[structopt(
        long,
        short,
        help = "",
        default_value = "ENDPOINT_URL",
        long_help = "Env variable with store URL from."
    )]
    url_variable: String,
    #[structopt(
        long,
        short,
        help = "",
        default_value = "ENDPOINT_AUTH",
        long_help = "Env variable with store authorization key."
    )]
    auth_variable: String,
    #[structopt(required = true, parse(from_os_str), help = "The bundle to upload.")]
    files: Vec<PathBuf>,
}

fn size_archive<R: Read + Seek>(za: &mut ZipArchive<R>) -> Result<u64> {
    let mut uncompressed = 0;
    for i in 0..za.len() {
        let zf = za.by_index_raw(i)?;
        uncompressed += zf.size();
    }
    Ok(uncompressed)
}

fn replace_version<P>(original_path: P, store_version: u32) -> Result<(PathBuf, String)>
where
    P: AsRef<Path>,
{
    let original_file = File::open(original_path.as_ref())?;

    let mut original_za = ZipArchive::new(original_file)?;
    let original_config = BundleConfig::from_archive(&mut original_za)?;

    let store_id = original_config.bundle.store_id.clone().ok_or_else(|| {
        anyhow!("All bundles uploaded to the store must have an assigned StoreID.")
    })?;

    let mut new_config = original_config.clone();
    new_config.bundle.version = Some(store_version.to_string());

    let new_path = format!("{}_{}.zip", store_id, store_version);
    let new_file = File::create(&new_path)?;

    let mut new_zw = ZipWriter::new(new_file);

    new_config.to_archive(&mut new_zw)?;
    for index in 0..original_za.len() {
        let mut original_zf = original_za.by_index(index)?;
        if original_zf.name().ends_with("bundle.ini") {
            continue;
        }
        let opts = FileOptions::default()
            .last_modified_time(original_zf.last_modified())
            .compression_method(original_zf.compression())
            .large_file(
                original_zf.size() >= (1 << 32) || original_zf.compressed_size() >= (1 << 32),
            )
            .unix_permissions(original_zf.unix_mode().unwrap_or(0o644));
        new_zw.start_file(original_zf.name(), opts)?;
        io::copy(&mut original_zf, &mut new_zw)?;
    }
    new_zw.finish()?;

    Ok((
        PathBuf::from(new_path),
        original_config
            .bundle
            .version
            .unwrap_or_else(|| store_version.to_string()),
    ))
}

fn main() -> Result<()> {
    let opts = Options::from_args();

    let client = Client::new();

    for file in opts.files.iter() {
        let (target_file, display_version) = replace_version(file, opts.store_version)?;

        let f = File::open(&target_file)?;
        let package_size = f.metadata()?.len();
        let mut za = ZipArchive::new(f)?;
        let b = BundleConfig::from_archive(&mut za)?;
        let installation_size = size_archive(&mut za)?;

        let store_id = b.bundle.store_id.clone().ok_or_else(|| {
            anyhow!("All bundles uploaded to the store must have an assigned StoreID.")
        })?;

        let release_date = format!(
            "{}",
            opts.release_date
                .unwrap_or_else(Utc::now)
                .format("%m-%d-%Y")
        );
        let release_note = opts
            .release_note
            .clone()
            .unwrap_or_else(|| "Automated release.".to_string());
        let additional_release_note = opts
            .additional_release_note
            .clone()
            .unwrap_or_else(|| "Automated release.".to_string());
        let version = b.bundle.version.ok_or_else(|| {
            anyhow!("Error creating archive: corrupt bundle.ini lacks Version field")
        })?;

        println!("Uploading:");
        println!("  Store ID:                {}", store_id);
        println!("  Display version:         {}", display_version);
        println!("  Version:                 {}", version);
        println!("  Original filename:       {}", file.to_string_lossy());
        println!(
            "  Filename for store:      {}",
            target_file.to_string_lossy()
        );
        println!("  Release note:            {}", release_note);
        println!("  Additional release note: {}", additional_release_note);
        println!("  Release date:            {}", release_date);
        println!("  Package size:            {}", package_size);
        println!("  Installation size:       {}", installation_size);

        let form = Form::new()
            .text("releaseDate", release_date)
            .text("packageSize", format!("{}", package_size))
            .text("installationSize", format!("{}", installation_size))
            .text("releaseNote", release_note)
            .text("additionalReleaseNote", additional_release_note)
            .text("version", version)
            .text("displayversion", display_version)
            .text("bundle_id", store_id)
            .file("file", target_file)?;

        let response = client
            .post(env::var(&opts.url_variable).map_err(|_| {
                anyhow!(
                    "{} environment variable not set; required to identify store",
                    opts.url_variable
                )
            })?)
            .header(
                "x-auth",
                env::var(&opts.auth_variable).map_err(|_| {
                    anyhow!(
                        "{} environment variable not set; required to connect to store",
                        opts.auth_variable
                    )
                })?,
            )
            .multipart(form)
            .send()?;

        let response = response.error_for_status()?;

        println!();
        println!("{}", response.text()?);
    }

    Ok(())
}
