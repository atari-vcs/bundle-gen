/*
 * Copyright 2022 Collabora, Ltd.
 *
 * SPDX-License-Identifier: MIT
 */
use std::path::PathBuf;

pub mod config;
pub mod generate;
pub mod ldcache;

/// An item waiting to be written to a bundle
#[derive(Clone, Debug)]
pub struct FileEntry {
    /// The location on disk of the item
    pub location: PathBuf,
    /// The item's destination path in the bundle
    pub name: String,
}
