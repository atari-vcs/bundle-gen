/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use anyhow::Result;
use bundle_gen::generate::generate;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(name = "FILE")]
    specification: String,
}

fn main() -> Result<()> {
    env_logger::init();

    let opt = Opt::from_args();
    Ok(generate(opt.specification).map(|bundle| println!("{}", bundle.to_string_lossy()))?)
}
