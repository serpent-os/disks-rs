// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::fmt;
use std::path::{Path, PathBuf};

use crate::{DEVFS_DIR, SYSFS_DIR, sysfs};

/// Represents a partition on a disk device
/// - Size in sectors
#[derive(Debug, Default)]
pub struct Partition {
    /// Name of the partition
    pub name: String,
    /// Partition number on the disk
    pub number: u32,
    /// Starting sector of the partition
    pub start: u64,
    /// Ending sector of the partition
    pub end: u64,
    /// Size of partition in sectors
    pub size: u64,
    /// Path to the partition node in sysfs
    pub node: PathBuf,
    /// Path to the partition device in /dev
    pub device: PathBuf,
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{name} {size:.2} GiB",
            name = self.name,
            size = self.size as f64 * 512.0 / (1024.0 * 1024.0 * 1024.0)
        )
    }
}

impl Partition {
    /// Creates a new Partition instance from a sysfs path and partition name.
    ///
    /// # Arguments
    /// * `sysroot` - Base path to sysfs
    /// * `name` - Name of the partition
    ///
    /// # Returns
    /// * `Some(Partition)` if partition exists and is valid
    /// * `None` if partition doesn't exist or is invalid
    pub fn from_sysfs_path(sysroot: &Path, name: &str) -> Option<Self> {
        let node = sysroot.join(SYSFS_DIR).join(name);
        let partition_no: u32 = sysfs::read(&node, "partition")?;
        let start = sysfs::read(&node, "start")?;
        let size = sysfs::read(&node, "size")?;
        Some(Self {
            name: name.to_owned(),
            number: partition_no,
            start,
            size,
            end: start + size,
            node,
            device: sysroot.join(DEVFS_DIR).join(name),
        })
    }
}
