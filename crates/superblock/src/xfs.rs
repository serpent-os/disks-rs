// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

//! XFS filesystem superblock parsing and handling
//!
//! This module provides functionality to parse and interact with XFS filesystem superblocks.
//! The XFS superblock contains critical metadata about the filesystem including:
//! - Basic filesystem parameters (block size, inode counts, etc)
//! - UUID and volume label information
//! - Feature flags and compatibility information
//! - Quota tracking data
//! - Log and realtime extent details

use crate::{Detection, UnicodeError};
use uuid::Uuid;
use zerocopy::*;

/// 64-bit block number for regular filesystem blocks
pub type RfsBlock = U64<BigEndian>;
/// 64-bit extent length for realtime blocks
pub type RtbXlen = U64<BigEndian>;
/// 64-bit filesystem block number
pub type FsBlock = U64<BigEndian>;
/// 64-bit inode number
pub type Ino = I64<BigEndian>;
/// 32-bit allocation group block number
pub type AgBlock = U32<BigEndian>;
/// 32-bit allocation group count
pub type AgCount = U32<BigEndian>;
/// 32-bit extent length
pub type ExtLen = U32<BigEndian>;
/// 64-bit log sequence number
pub type Lsn = I64<BigEndian>;

/// Maximum length of XFS volume label (12 bytes)
pub const MAX_LABEL_LEN: usize = 12;

/// XFS superblock structure, containing filesystem metadata and parameters
///
/// This structure maps directly to the on-disk format of an XFS superblock.
/// All multi-byte integer fields are stored in big-endian byte order.
#[derive(FromBytes, Debug)]
#[repr(C, align(8))]
pub struct Xfs {
    /// Magic number, must contain 'XFSB'
    pub magicnum: U32<BigEndian>,
    /// Filesystem block size in bytes
    pub blocksize: U32<BigEndian>,
    /// Number of blocks in data subvolume
    pub dblocks: RfsBlock,
    /// Number of blocks in realtime subvolume
    pub rblocks: RfsBlock,
    /// Number of realtime extents
    pub rextents: RtbXlen,
    /// Filesystem UUID
    pub uuid: [u8; 16],
    /// Starting block of log if internal log
    pub logstart: FsBlock,
    /// Root directory inode number
    pub rootino: Ino,
    /// Realtime bitmap inode
    pub rbmino: Ino,
    /// Realtime summary inode
    pub rsumino: Ino,
    /// Realtime extent size in blocks
    pub rextsize: AgBlock,
    /// Blocks per allocation group
    pub agblocks: AgBlock,
    /// Number of allocation groups
    pub agcount: AgCount,
    /// Number of realtime bitmap blocks
    pub rbmblocks: ExtLen,
    /// Number of log blocks
    pub logblocks: ExtLen,
    /// Filesystem version number
    pub versionnum: U16<BigEndian>,
    /// Sector size in bytes
    pub sectsize: U16<BigEndian>,
    /// Inode size in bytes
    pub inodesize: U16<BigEndian>,
    /// Inodes per block
    pub inopblock: U16<BigEndian>,
    /// Filesystem volume name/label
    pub fname: [u8; MAX_LABEL_LEN],
    /// Log2 of blocksize
    pub blocklog: u8,
    /// Log2 of sector size
    pub sectlog: u8,
    /// Log2 of inode size
    pub inodelog: u8,
    /// Log2 of inodes per block
    pub inopblog: u8,
    /// Log2 of blocks per allocation group
    pub agblklog: u8,
    /// Log2 of realtime extents
    pub rextslog: u8,
    /// Filesystem being created flag
    pub inprogress: u8,
    /// Max % of fs for inodes
    pub imax_pct: u8,

    /// Number of inodes allocated
    pub icount: U64<BigEndian>,
    /// Number of free inodes
    pub ifree: U64<BigEndian>,
    /// Number of free data blocks
    pub fdblocks: U64<BigEndian>,
    /// Number of free realtime extents
    pub frextents: U64<BigEndian>,

    /// User quota inode
    pub uquotino: Ino,
    /// Group quota inode
    pub gquotino: Ino,
    /// Quota flags
    pub qflags: U16<BigEndian>,
    /// Flags
    pub flags: u8,
    /// Shared version number
    pub shared_vn: u8,
    /// Inode chunk alignment
    pub inoalignment: ExtLen,
    /// Stripe or RAID unit
    pub unit: U32<BigEndian>,
    /// Stripe or RAID width
    pub width: U32<BigEndian>,
    /// Log2 of dir block size
    pub dirblklog: u8,
    /// Log2 of log sector size
    pub logsectlog: u8,
    /// Log sector size
    pub logsectsize: U16<BigEndian>,
    /// Log stripe unit size
    pub logsunit: U32<BigEndian>,
    /// Version 2 features
    pub features2: U32<BigEndian>,

    /// Bad features mask
    pub bad_features: U32<BigEndian>,

    /// Compatible feature flags
    pub features_compat: U32<BigEndian>,
    /// Read-only compatible feature flags
    pub features_ro_cmopat: U32<BigEndian>,
    /// Incompatible feature flags
    pub features_incompat: U32<BigEndian>,
    /// Log incompatible feature flags
    pub features_log_incompat: U32<BigEndian>,

    /// Superblock checksum
    pub crc: U32<BigEndian>,
    /// Sparse inode alignment
    pub spino_align: ExtLen,

    /// Project quota inode
    pub pquotino: Ino,
    /// Last write sequence
    pub lsn: Lsn,
    /// Metadata UUID
    pub meta_uuid: [u8; 16],
}

/// XFS superblock magic number ('XFSB' in ASCII)
pub const MAGIC: U32<BigEndian> = U32::new(0x58465342);

impl Xfs {
    /// Returns the filesystem UUID as a properly formatted string
    pub fn uuid(&self) -> Result<String, UnicodeError> {
        Ok(Uuid::from_bytes(self.uuid).hyphenated().to_string())
    }

    /// Returns the volume label as a UTF-8 string, trimming any null termination
    pub fn label(&self) -> Result<String, UnicodeError> {
        Ok(std::str::from_utf8(&self.fname)?.trim_end_matches('\0').to_owned())
    }
}

impl Detection for Xfs {
    type Magic = U32<BigEndian>;

    const OFFSET: u64 = 0x0;

    const MAGIC_OFFSET: u64 = 0x0;

    const SIZE: usize = std::mem::size_of::<Xfs>();

    fn is_valid_magic(magic: &Self::Magic) -> bool {
        *magic == MAGIC
    }
}
