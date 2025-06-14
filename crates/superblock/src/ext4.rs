// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

//! EXT4 superblock handling
//!
//! This module provides functionality for parsing and interacting with EXT4 filesystem superblocks.
//! The superblock contains critical metadata about the filesystem including UUID, volume label,
//! and various configuration parameters.

use crate::{Detection, UnicodeError};
use uuid::Uuid;
use zerocopy::*;

/// EXT4 Superblock definition that mirrors the on-disk format used by the Linux kernel.
/// Contains metadata and configuration for an EXT4 filesystem.
#[derive(Debug, FromBytes)]
#[repr(C)]
pub struct Ext4 {
    /// Total count of inodes in filesystem
    pub inodes_count: U32<LittleEndian>,
    /// Total count of blocks (lower 32-bits)
    pub block_counts_lo: U32<LittleEndian>,
    /// Reserved block count (lower 32-bits)
    pub r_blocks_count_lo: U32<LittleEndian>,
    /// Free blocks count (lower 32-bits)
    pub free_blocks_count_lo: U32<LittleEndian>,
    /// Free inodes count
    pub free_inodes_count: U32<LittleEndian>,
    /// First data block location
    pub first_data_block: U32<LittleEndian>,
    /// Block size is 2 ^ (10 + log_block_size)
    pub log_block_size: U32<LittleEndian>,
    /// Cluster size is 2 ^ (10 + log_cluster_size)
    pub log_cluster_size: U32<LittleEndian>,
    /// Number of blocks per group
    pub blocks_per_group: U32<LittleEndian>,
    /// Number of clusters per group
    pub clusters_per_group: U32<LittleEndian>,
    /// Number of inodes per group
    pub inodes_per_group: U32<LittleEndian>,
    /// Mount time
    pub m_time: U32<LittleEndian>,
    /// Write time
    pub w_time: U32<LittleEndian>,
    /// Number of mounts since last consistency check
    pub mnt_count: U16<LittleEndian>,
    /// Maximum mounts allowed between checks
    pub max_mnt_count: U16<LittleEndian>,
    /// Magic signature (0xEF53)
    pub magic: U16<LittleEndian>,
    /// Filesystem state flags
    pub state: U16<LittleEndian>,
    /// Behavior when detecting errors
    pub errors: U16<LittleEndian>,
    /// Minor revision level
    pub minor_rev_level: U16<LittleEndian>,
    /// Time of last consistency check
    pub lastcheck: U32<LittleEndian>,
    /// Maximum time between checks
    pub checkinterval: U32<LittleEndian>,
    /// OS that created the filesystem
    pub creator_os: U32<LittleEndian>,
    /// Revision level
    pub rev_level: U32<LittleEndian>,
    /// Default uid for reserved blocks
    pub def_resuid: U16<LittleEndian>,
    /// Default gid for reserved blocks
    pub def_resgid: U16<LittleEndian>,
    /// First non-reserved inode
    pub first_ino: U32<LittleEndian>,
    /// Size of inode structure
    pub inode_size: U16<LittleEndian>,
    /// Block group number of this superblock
    pub block_group_nr: U16<LittleEndian>,
    /// Compatible feature set flags
    pub feature_compat: U32<LittleEndian>,
    /// Incompatible feature set flags
    pub feature_incompat: U32<LittleEndian>,
    /// Read-only compatible feature set flags
    pub feature_ro_compat: U32<LittleEndian>,
    /// 128-bit filesystem identifier
    pub uuid: [u8; 16],
    /// Volume name
    pub volume_name: [u8; 16],
    /// Directory where last mounted
    pub last_mounted: [u8; 64],
    /// For compression
    pub algorithm_usage_bitmap: U32<LittleEndian>,
    /// Number of blocks to preallocate
    pub prealloc_blocks: u8,
    /// Number of blocks to preallocate for directories
    pub prealloc_dir_blocks: u8,
    /// Reserved GDT blocks for online filesystem growth
    pub reserved_gdt_blocks: U16<LittleEndian>,
    /// Journal UUID
    pub journal_uuid: [u8; 16],
    /// Journal inode
    pub journal_inum: U32<LittleEndian>,
    /// Journal device
    pub journal_dev: U32<LittleEndian>,
    /// Head of list of inodes to delete
    pub last_orphan: U32<LittleEndian>,
    /// HTREE hash seed
    pub hash_seed: [U32<LittleEndian>; 4],
    /// Default hash version to use
    pub def_hash_version: u8,
    /// Journal backup type
    pub jnl_backup_type: u8,
    /// Group descriptor size
    pub desc_size: U16<LittleEndian>,
    /// Default mount options
    pub default_mount_opts: U32<LittleEndian>,
    /// First metablock block group
    pub first_meta_bg: U32<LittleEndian>,
    /// When the filesystem was created
    pub mkfs_time: U32<LittleEndian>,
    /// Journal backup
    pub jnl_blocks: [U32<LittleEndian>; 17],
    /// High 32-bits of block count
    pub blocks_count_hi: U32<LittleEndian>,
    /// High 32-bits of free block count
    pub free_blocks_count_hi: U32<LittleEndian>,
    /// Minimum inode extra size
    pub min_extra_isize: U16<LittleEndian>,
    /// Desired inode extra size
    pub want_extra_isize: U16<LittleEndian>,
    /// Miscellaneous flags
    pub flags: U32<LittleEndian>,
    /// RAID stride
    pub raid_stride: U16<LittleEndian>,
    /// MMP update interval
    pub mmp_update_interval: U16<LittleEndian>,
    /// Multi-mount protection block
    pub mmp_block: U64<LittleEndian>,
    /// RAID stripe width
    pub raid_stripe_width: U32<LittleEndian>,
    /// Log groups per flex block
    pub log_groups_per_flex: u8,
    /// Metadata checksum type
    pub checksum_type: u8,
    /// Reserved padding
    pub reserved_pad: U16<LittleEndian>,
    /// Number of KiB written
    pub kbytes_written: U64<LittleEndian>,
    /// Snapshot inode number
    pub snapshot_inum: U32<LittleEndian>,
    /// Snapshot ID
    pub snapshot_id: U32<LittleEndian>,
    /// Reserved blocks for snapshot
    pub snapshot_r_blocks_count: U64<LittleEndian>,
    /// Snapshot list ID
    pub snapshot_list: U32<LittleEndian>,
    /// Error count
    pub error_count: U32<LittleEndian>,
    /// First error time
    pub first_error_time: U32<LittleEndian>,
    /// First error inode
    pub first_error_inod: U32<LittleEndian>,
    /// First error block
    pub first_error_block: U64<LittleEndian>,
    /// First error function
    pub first_error_func: [u8; 32],
    /// First error line number
    pub first_error_line: U32<LittleEndian>,
    /// Last error time
    pub last_error_time: U32<LittleEndian>,
    /// Last error inode
    pub last_error_inod: U32<LittleEndian>,
    /// Last error line number
    pub last_error_line: U32<LittleEndian>,
    /// Last error block
    pub last_error_block: U64<LittleEndian>,
    /// Last error function
    pub last_error_func: [u8; 32],
    /// Mount options in string form
    pub mount_opts: [u8; 64],
    /// User quota inode
    pub usr_quota_inum: U32<LittleEndian>,
    /// Group quota inode
    pub grp_quota_inum: U32<LittleEndian>,
    /// Overhead blocks/clusters
    pub overhead_clusters: U32<LittleEndian>,
    /// Reserved for future expansion
    pub reserved: [U32<LittleEndian>; 108],
    /// Superblock checksum
    pub checksum: U32<LittleEndian>,
}

/// Magic number that identifies an EXT4 superblock
pub const MAGIC: U16<LittleEndian> = U16::new(0xEF53);

/// Start position of superblock in filesystem
pub const START_POSITION: u64 = 1024;

impl Detection for Ext4 {
    type Magic = U16<LittleEndian>;

    const OFFSET: u64 = START_POSITION;

    const MAGIC_OFFSET: u64 = START_POSITION + 0x38;

    const SIZE: usize = std::mem::size_of::<Ext4>();

    fn is_valid_magic(magic: &Self::Magic) -> bool {
        *magic == MAGIC
    }
}

impl Ext4 {
    /// Return the encoded UUID for this superblock
    pub fn uuid(&self) -> Result<String, UnicodeError> {
        Ok(Uuid::from_bytes(self.uuid).hyphenated().to_string())
    }

    /// Return the volume label as valid utf8
    pub fn label(&self) -> Result<String, UnicodeError> {
        Ok(std::str::from_utf8(&self.volume_name)?.into())
    }
}
