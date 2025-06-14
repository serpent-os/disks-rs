// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

//! Loopback device enumeration and handling.
//!
//! Loopback devices in Linux are block devices that map files to block devices.
//! This module handles enumeration and management of these devices,
//! which appear as `/dev/loop*` block devices.

use std::path::{Path, PathBuf};

use crate::{BasicDisk, DEVFS_DIR, DiskInit, SYSFS_DIR, sysfs};

/// Represents a loop device.
#[derive(Debug)]
pub struct Device {
    /// The device name (e.g. "loop0", "loop1")
    name: String,

    /// Path to the device in /dev
    device: PathBuf,

    /// Optional backing file path
    file: Option<PathBuf>,

    /// Optional disk device if the loop device is backed by a disk
    disk: Option<BasicDisk>,
}

impl Device {
    /// Creates a new Device instance from a sysfs path if the device name matches loop device pattern.
    ///
    /// # Arguments
    ///
    /// * `sysroot` - The root path of the sysfs filesystem
    /// * `name` - The device name to check (e.g. "loop0", "loop1")
    ///
    /// # Returns
    ///
    /// * `Some(Device)` if the name matches loop pattern (starts with "loop" followed by numbers)
    /// * `None` if the name doesn't match or the device can't be initialized
    pub fn from_sysfs_path(sysroot: &Path, name: &str) -> Option<Self> {
        let matching = name.starts_with("loop") && name[4..].chars().all(char::is_numeric);
        let node = sysroot.join(SYSFS_DIR).join(name);
        let file = sysfs::read::<PathBuf>(&node, "loop/backing_file");
        let disk = file.as_ref().and_then(|_| BasicDisk::from_sysfs_path(sysroot, name));
        if matching {
            Some(Self {
                name: name.to_owned(),
                device: PathBuf::from("/").join(DEVFS_DIR).join(name),
                file,
                disk,
            })
        } else {
            None
        }
    }

    /// Creates a new Device instance from a device path.
    pub fn from_device_path(device: &Path) -> Option<Self> {
        let name = device.file_name()?.to_string_lossy().to_string();
        Self::from_sysfs_path(&PathBuf::from("/"), &name)
    }

    /// Returns the device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the device path.
    pub fn device_path(&self) -> &Path {
        &self.device
    }

    /// Returns the backing file path.
    pub fn file_path(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Returns the disk device if the loop device is backed by a disk.
    pub fn disk(&self) -> Option<&BasicDisk> {
        self.disk.as_ref()
    }
}
