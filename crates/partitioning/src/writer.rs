// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
// SPDX-FileCopyrightText: Copyright © 2025 AerynOS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    fs,
    io::{self, Seek, Write},
};

use disks::BlockDevice;
use gpt::{GptConfig, mbr, partition_types};
use thiserror::Error;

use crate::{
    GptAttributes, blkpg,
    planner::{Change, Planner},
};
const SECTOR_SIZE: u64 = 512;

/// Errors that can occur when writing changes to disk
#[derive(Debug, Error)]
pub enum WriteError {
    // A blkpg error
    #[error("error syncing partitions: {0}")]
    Blkpg(#[from] blkpg::Error),

    /// A partition ID was used multiple times
    #[error("Duplicate partition ID: {0}")]
    DuplicatePartitionId(u32),

    /// Error from GPT library
    #[error("GPT error: {0}")]
    Gpt(#[from] gpt::GptError),

    /// Error from MBR handling
    #[error("GPT error: {0}")]
    Mbr(#[from] gpt::mbr::MBRError),

    /// Underlying I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// A writer that applies the layouts from the Planner to the disk.
pub struct DiskWriter<'a> {
    /// The block device to write to
    pub device: &'a BlockDevice,
    /// The planner containing the changes to apply
    pub planner: &'a Planner,
}

/// Zero out a specific region of the disk
fn zero_region<W: Write + Seek>(writer: &mut W, offset: u64, size: u64) -> io::Result<()> {
    let zeros = [0u8; 65_536];
    writer.seek(std::io::SeekFrom::Start(offset))?;
    let chunks = (size / 65_536) as usize;
    for _ in 0..chunks {
        writer.write_all(&zeros)?;
    }
    // Handle any remaining bytes
    let remainder = size % 65_536;
    if remainder > 0 {
        writer.write_all(&zeros[..remainder as usize])?;
    }
    writer.flush()?;
    Ok(())
}

/// Zero out disk headers by wiping first 2MiB of the disk
fn zero_disk_headers<W: Write + Seek>(writer: &mut W) -> io::Result<()> {
    // Clear first 2MiB to wipe all common boot structures
    zero_region(writer, 0, 2 * 1024 * 1024)
}

/// Zero out up to 2MiB of a partition by writing 32 * 64KiB blocks
/// Zero out up to 2MiB of a region by writing 32 * 64KiB blocks
fn zero_partition_prefix<W: Write + Seek>(writer: &mut W, offset: u64, size: u64) -> io::Result<()> {
    let to_zero = std::cmp::min(size, 2 * 1024 * 1024); // 2MiB max
    zero_region(writer, offset, to_zero)
}

impl<'a> DiskWriter<'a> {
    /// Create a new DiskWriter.
    pub fn new(device: &'a BlockDevice, planner: &'a Planner) -> Self {
        Self { device, planner }
    }

    /// Simulate changes without writing to disk
    pub fn simulate(&self) -> Result<(), WriteError> {
        let mut device = fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(self.device.device())?;
        self.validate_changes()?;
        self.apply_changes(&mut device, false)?;
        Ok(())
    }

    /// Actually write changes to disk
    pub fn write(&self) -> Result<(), WriteError> {
        let mut device = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.device.device())?;

        self.validate_changes()?;
        self.apply_changes(&mut device, true)?;
        device.flush()?;
        Ok(())
    }

    /// Validate all planned changes before applying them by checking:
    /// - Device size matches the planned size
    /// - No duplicate partition IDs exist
    fn validate_changes(&self) -> Result<(), WriteError> {
        // Verify partition IDs don't conflict
        let mut used_ids = std::collections::HashSet::new();
        for change in self.planner.changes() {
            match change {
                Change::AddPartition { partition_id, .. } => {
                    if !used_ids.insert(*partition_id) {
                        return Err(WriteError::DuplicatePartitionId(*partition_id));
                    }
                }
                Change::DeletePartition { partition_id, .. } => {
                    used_ids.remove(partition_id);
                }
            }
        }

        Ok(())
    }

    /// Apply the changes to disk by:
    /// - Creating or opening the GPT table
    /// - Applying each change in sequence
    fn apply_changes(&self, device: &mut fs::File, writable: bool) -> Result<(), WriteError> {
        // Remove known partitions pre wipe
        if writable {
            blkpg::remove_kernel_partitions(self.device.device())?;
        }

        let mut zero_regions = vec![];

        let mut gpt_table = if self.planner.wipe_disk() {
            if writable {
                // Zero out headers including potential ISO structures
                zero_disk_headers(device)?;

                // Convert total bytes to LBA sectors, subtract 1 as per GPT spec
                let total_lba = self.device.size() / SECTOR_SIZE;
                let mbr = mbr::ProtectiveMBR::with_lb_size(
                    u32::try_from(total_lba.saturating_sub(1)).unwrap_or(0xFF_FF_FF_FF),
                );
                eprintln!("size is {}", self.device.size());
                mbr.overwrite_lba0(device)?;
            }

            let mut c = GptConfig::default()
                .writable(writable)
                .logical_block_size(gpt::disk::LogicalBlockSize::Lb512)
                .create_from_device(device, None)?;

            if writable {
                c.write_inplace()?;
            }
            c
        } else {
            GptConfig::default().writable(writable).open_from_device(device)?
        };

        let layout = self.planner.current_layout();
        let changes = self.planner.changes();

        eprintln!("Changes: {changes:?}");

        for change in changes {
            match change {
                Change::DeletePartition {
                    partition_id,
                    original_index,
                } => {
                    if let Some(id) = gpt_table.remove_partition(*partition_id) {
                        println!("Deleted partition {partition_id} (index {original_index}): {id:?}");
                    }
                }
                Change::AddPartition {
                    start,
                    end,
                    partition_id,
                    attributes,
                } => {
                    // Convert byte offsets to LBA sectors
                    let start_lba = *start / SECTOR_SIZE;
                    let size_bytes = *end - *start;
                    let size_lba = size_bytes / SECTOR_SIZE;
                    let (part_type, part_name) = match attributes.as_ref().and_then(|a| a.table.as_gpt()) {
                        Some(GptAttributes { type_guid, name, .. }) => {
                            (type_guid.clone(), name.clone().unwrap_or_default())
                        }
                        None => (partition_types::BASIC, "".to_string()),
                    };

                    eprintln!(
                        "Converting partition: bytes {}..{} to LBA {}..{}",
                        start,
                        end,
                        start_lba,
                        start_lba + size_lba
                    );
                    let id =
                        gpt_table.add_partition_at(&part_name, *partition_id, start_lba, size_lba, part_type, 0)?;
                    println!("Added partition {partition_id}: {id:?}");
                    // Store start and size for zeroing
                    if writable {
                        zero_regions.push((*start, *end));
                    }
                }
            }
        }

        eprintln!("### GPT is now: {gpt_table:?}");

        for region in layout.iter() {
            eprintln!(
                "Region at: {:?}",
                region.partition_id.map(|i| self.device.partition_path(i as usize))
            );
        }

        // Consume and sync the GPT table
        if writable {
            let original = gpt_table.write()?;
            original.sync_all()?;

            for (start, end) in zero_regions {
                zero_partition_prefix(original, start, end - start)?;
            }

            blkpg::create_kernel_partitions(self.device.device())?;
        }

        Ok(())
    }
}
