// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

//! BTRFS superblock handling
//!
//! This module provides functionality for reading and parsing BTRFS filesystem superblocks,
//! which contain critical metadata about the filesystem including UUIDs and labels.

use crate::{Detection, UnicodeError};
use uuid::Uuid;
use zerocopy::*;

/// BTRFS superblock definition that matches the on-disk format used by the Linux kernel.
///
/// The superblock contains critical filesystem metadata including:
/// - Filesystem UUID and label
/// - Size and usage information
/// - Root tree locations
/// - Compatibility flags
#[derive(FromBytes, Debug)]
#[repr(C)]
pub struct Btrfs {
    /// Checksum of the superblock data
    pub csum: [u8; 32],
    /// Filesystem UUID
    pub fsid: [u8; 16],
    /// Physical byte number where this copy of the superblock is located
    pub bytenr: U64<LittleEndian>,
    /// Superblock flags
    pub flags: U64<LittleEndian>,
    /// Magic number identifying this as a BTRFS superblock
    pub magic: U64<LittleEndian>,
    /// Transaction ID of the filesystem
    pub generation: U64<LittleEndian>,
    /// Logical address of the root tree root
    pub root: U64<LittleEndian>,
    /// Logical address of the chunk tree root
    pub chunk_root: U64<LittleEndian>,
    /// Logical address of the log tree root
    pub log_root: U64<LittleEndian>,
    /// Transaction ID of the log tree
    pub log_root_transid: U64<LittleEndian>,
    /// Total size of the filesystem in bytes
    pub total_bytes: U64<LittleEndian>,
    /// Number of bytes used
    pub bytes_used: U64<LittleEndian>,
    /// Object ID of the root directory
    pub root_dir_objectid: U64<LittleEndian>,
    /// Number of devices making up the filesystem
    pub num_devices: U64<LittleEndian>,
    /// Size of a sector in bytes
    pub sectorsize: U32<LittleEndian>,
    /// Size of nodes in the filesystem trees
    pub nodesize: U32<LittleEndian>,
    /// Size of leaf nodes in the filesystem trees
    pub leafsize: U32<LittleEndian>,
    /// Stripe size for the filesystem
    pub stripesize: U32<LittleEndian>,
    /// Size of the system chunk array
    pub sys_chunk_array_size: U32<LittleEndian>,
    /// Generation of the chunk tree
    pub chunk_root_generation: U64<LittleEndian>,
    /// Compatible feature flags
    pub compat_flags: U64<LittleEndian>,
    /// Compatible read-only feature flags
    pub compat_ro_flags: U64<LittleEndian>,
    /// Incompatible feature flags
    pub incompat_flags: U64<LittleEndian>,
    /// Checksum algorithm type
    pub csum_type: U16<LittleEndian>,
    /// Level of the root tree
    pub root_level: u8,
    /// Level of the chunk tree
    pub chunk_root_level: u8,
    /// Level of the log tree
    pub log_root_level: u8,
    /// Device information
    pub dev_item: [u8; 98],
    /// Volume label
    pub label: [u8; 256],
    /// Cache generation number
    pub cache_generation: U64<LittleEndian>,
    /// UUID tree generation
    pub uuid_tree_generation: U64<LittleEndian>,
    /// Metadata UUID for the filesystem
    pub metadata_uuid: [u8; 16],
    /// Number of global root entries
    pub nr_global_roots: U64<LittleEndian>,
    /// Reserved for future use
    pub reserved: [u8; 32],
    /// System chunk array data
    pub sys_chunk_array: [u8; 2048],
    /// Backup copy of root tree info
    pub root_backup: [u8; 256],
}

/// Offset where the BTRFS superblock starts (65536 bytes)
pub const START_POSITION: u64 = 0x10000;

/// Magic number identifying a BTRFS superblock ("_BHRfS_M")
pub const MAGIC: U64<LittleEndian> = U64::new(0x4D5F53665248425F);

impl Detection for Btrfs {
    type Magic = U64<LittleEndian>;

    const OFFSET: u64 = START_POSITION;

    const MAGIC_OFFSET: u64 = START_POSITION + 0x40;

    const SIZE: usize = std::mem::size_of::<Btrfs>();

    fn is_valid_magic(magic: &Self::Magic) -> bool {
        *magic == MAGIC
    }
}

impl Btrfs {
    /// Return the encoded UUID for this superblock as a string
    pub fn uuid(&self) -> Result<String, UnicodeError> {
        Ok(Uuid::from_bytes(self.fsid).hyphenated().to_string())
    }

    /// Return the volume label as a string
    pub fn label(&self) -> Result<String, UnicodeError> {
        Ok(std::str::from_utf8(&self.label)?.trim_end_matches('\0').to_owned())
    }
}
