// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

//! # LUKS2 superblock support
//!
//! This module provides functionality for reading and parsing LUKS2 (Linux Unified Key Setup 2)
//! superblocks and their associated metadata.
//!
//! LUKS2 is a disk encryption format that uses the dm-crypt subsystem. It stores metadata
//! like encryption parameters, key slots and segment information in JSON format.
//!
//! ## Format Details
//!
//! The LUKS2 header contains:
//! - Magic number to identify LUKS2 format
//! - Version number
//! - Header size and offset
//! - UUID and label for volume identification
//! - Checksum algorithm and salt for validation
//! - JSON metadata area containing encryption parameters
//!

use std::{
    io::{Read, Seek},
    ops::Sub,
};

use snafu::ResultExt;
use zerocopy::*;

use super::{ConfigError, InvalidJsonSnafu, InvalidUtf8Snafu, IoSnafu, Luks2Config};
use crate::{Detection, UnicodeError};

/// Length of the magic number field in bytes
pub const MAGIC_LEN: usize = 6;
/// Length of the label field in bytes
pub const LABEL_LEN: usize = 48;
/// Length of the checksum algorithm field in bytes
pub const CHECKSUM_ALG_LEN: usize = 32;
/// Length of the salt field in bytes
pub const SALT_LEN: usize = 64;
/// Length of the UUID field in bytes
pub const UUID_LEN: usize = 40;
/// Length of the checksum field in bytes
pub const CHECKSUM_LEN: usize = 64;

/// LUKS2 on-disk header format
///
/// Per the `cryptsetup` docs for dm-crypt backed LUKS2, header is at first byte.
/// The header contains metadata about the encrypted volume including magic number,
/// version, checksums and JSON configuration.
#[derive(FromBytes, Unaligned, Debug)]
#[repr(C, packed)]
pub struct Luks2 {
    /// Magic number identifying LUKS2 format
    pub magic: [u8; MAGIC_LEN],
    /// LUKS format version
    pub version: U16<BigEndian>,
    /// Size of the header in bytes
    pub hdr_size: U64<BigEndian>,
    /// Header sequence ID for rewrite protection
    pub seqid: U64<BigEndian>,
    /// Volume label
    pub label: [u8; LABEL_LEN],
    /// Checksum algorithm identifier
    pub checksum_alg: [u8; CHECKSUM_ALG_LEN],
    /// Salt used for checksum
    pub salt: [u8; SALT_LEN],
    /// Volume UUID
    pub uuid: [u8; UUID_LEN],
    /// Subsystem label
    pub subsystem: [u8; LABEL_LEN],
    /// Secondary header offset
    pub hdr_offset: U64<BigEndian>,
    /// Padding bytes
    pub padding: [u8; 184],
    /// Header checksum
    pub csum: [u8; CHECKSUM_LEN],
    /// Additional padding to 4096 bytes
    pub padding4096: [u8; 7 * 512],
}

/// Magic number constants for LUKS2 format identification
pub struct MagicMatch;

impl MagicMatch {
    /// Standard LUKS2 magic number
    pub const LUKS2: [u8; MAGIC_LEN] = [b'L', b'U', b'K', b'S', 0xba, 0xbe];
    /// Alternative LUKS2 magic number (reversed)
    pub const SKUL2: [u8; MAGIC_LEN] = [b'S', b'K', b'U', b'L', 0xba, 0xbe];
}

impl Detection for Luks2 {
    type Magic = [u8; MAGIC_LEN];

    const OFFSET: u64 = 0;

    const MAGIC_OFFSET: u64 = 0;

    const SIZE: usize = std::mem::size_of::<Luks2>();

    fn is_valid_magic(magic: &Self::Magic) -> bool {
        *magic == MagicMatch::LUKS2 || *magic == MagicMatch::SKUL2
    }
}

impl Luks2 {
    /// Get the UUID of the LUKS2 volume
    ///
    /// Note: LUKS2 stores string UUID rather than 128-bit sequence
    pub fn uuid(&self) -> Result<String, UnicodeError> {
        Ok(std::str::from_utf8(&self.uuid)?.trim_end_matches('\0').to_owned())
    }

    /// Get the label of the LUKS2 volume
    ///
    /// Note: Label is often empty, set in config instead
    pub fn label(&self) -> Result<String, UnicodeError> {
        Ok(std::str::from_utf8(&self.label)?.trim_end_matches('\0').to_owned())
    }

    /// Read and parse the JSON configuration areas from the LUKS2 header
    ///
    /// # Arguments
    ///
    /// * `reader` - Any type implementing Read trait to read the JSON data
    ///
    /// # Returns
    ///
    /// Returns parsed Luks2Config on success, Error on failure
    pub fn read_config<R: Read + Seek>(&self, reader: &mut R) -> Result<Luks2Config, ConfigError> {
        let mut json_data = vec![0u8; self.hdr_size.get().sub(4096) as usize];
        // Skip the header and read the JSON data
        reader
            .seek(std::io::SeekFrom::Start(std::mem::size_of::<Luks2>() as u64))
            .context(IoSnafu)?;
        reader.read_exact(&mut json_data).context(IoSnafu)?;

        // clip the json_data at the first nul byte
        let raw_input = std::str::from_utf8(&json_data)
            .context(InvalidUtf8Snafu)?
            .trim_end_matches('\0');
        let config: Luks2Config = serde_json::from_str(raw_input).context(InvalidJsonSnafu)?;
        Ok(config)
    }
}
