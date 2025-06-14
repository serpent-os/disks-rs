// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0
//
//! Disk partition planning and validation
//!
//! This module provides functionality for planning changes to disk partition layouts in a safe,
//! validated way. It allows you to:
//!
//! - Plan new partition additions with proper alignment
//! - Remove existing partitions
//! - Track and undo changes
//! - Validate that changes won't conflict with existing partitions

use disks::{BlockDevice, align_down, align_up, format_position, format_size, is_aligned};
use log::{debug, warn};
use std::collections::VecDeque;
use thiserror::Error;

use crate::PartitionAttributes;

/// Errors that can occur while planning partition changes
///
/// These errors help prevent invalid partition layouts by catching problems
/// early in the planning phase.
#[derive(Debug, Error)]
pub enum PlanError {
    #[error("Region {start}..{end} overlaps with existing partition")]
    RegionOverlap { start: u64, end: u64 },
    #[error("Region {start}..{end} exceeds disk bounds")]
    RegionOutOfBounds { start: u64, end: u64 },
    #[error("No free regions available")]
    NoFreeRegions,
}

/// A planned modification to the disk's partition layout
///
/// Changes are tracked in sequence and can be undone using [`Planner::undo()`].
/// Each change is validated when added to ensure it won't create an invalid
/// disk layout.
#[derive(Debug, Clone)]
pub enum Change {
    /// Add a new partition
    AddPartition {
        start: u64,
        end: u64,
        partition_id: u32,
        attributes: Option<PartitionAttributes>,
    },
    /// Delete an existing partition
    DeletePartition { original_index: usize, partition_id: u32 },
}

/// A disk partitioning planner.
#[derive(Debug, Clone)]
pub struct Planner {
    /// First usable LBA position on disk in bytes
    usable_start: u64,
    /// Last usable LBA position on disk in bytes
    usable_end: u64,
    /// Stack of changes that can be undone
    changes: VecDeque<Change>,
    /// Original partition layout for reference
    original_regions: Vec<Region>,
    /// Track original partition IDs
    original_partition_ids: Vec<u32>,
    /// Next available partition ID for new partitions
    next_partition_id: u32,

    wipe_disk: bool,
}

/// A contiguous region of disk space defined by absolute start and end positions
///
/// Used to represent both existing partitions and planned partition changes.
/// All positions are measured in bytes from the start of the disk.
///
/// # Examples
///
/// ```
/// use partitioning::planner::Region;
/// let region = Region::new(0, 1024 * 1024); // 1MiB partition at start of disk
/// assert_eq!(region.size(), 1024 * 1024);
/// ```
#[derive(Debug, Clone)]
pub struct Region {
    /// The absolute start position of this region in bytes
    pub start: u64,

    /// The absolute end position of this region in bytes
    pub end: u64,

    /// The partition ID of this region if it represents a partition
    pub partition_id: Option<u32>,

    pub attributes: Option<PartitionAttributes>,
}

/// partitions aligned to 1MiB boundaries. This helps ensure optimal
/// performance and compatibility.
pub const PARTITION_ALIGNMENT: u64 = 1024 * 1024;

/// Represents a contiguous region on disk between two absolute positions.
/// Both start and end are absolute positions in bytes from the beginning of the disk.
/// For example, a 1MB partition starting at the beginning of the disk would have
/// start=0 and end=1048576.
impl Region {
    /// Create a new region with the given bounds
    pub fn new(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            partition_id: None,
            attributes: None,
        }
    }

    /// Get the size of this region in bytes
    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    /// Check if this region overlaps with another
    pub fn overlaps_with(&self, other: &Region) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Get a human readable description of this region
    pub fn describe(&self, disk_size: u64) -> String {
        format!(
            "{} at {}..{}",
            format_size(self.size()),
            format_position(self.start, disk_size),
            format_position(self.end, disk_size)
        )
    }
}

impl Change {
    /// Get a human readable description of this change
    pub fn describe(&self, disk_size: u64) -> String {
        match self {
            Change::AddPartition {
                start,
                end,
                partition_id,
                ..
            } => {
                format!(
                    "Add new partition #{}: {} ({} at {})",
                    partition_id,
                    format_size(end - start),
                    Region::new(*start, *end).describe(disk_size),
                    format_position(*start, disk_size)
                )
            }
            Change::DeletePartition {
                original_index,
                partition_id,
            } => {
                format!("Delete partition #{} (index {})", partition_id, original_index + 1)
            }
        }
    }
}

impl Planner {
    /// Creates a new partitioning planner for the given disk.
    pub fn new(device: &BlockDevice) -> Self {
        debug!("Creating new partition planner for device of size {}", device.size());

        // Extract original regions and partition IDs from device
        let mut original_regions = Vec::new();
        let mut original_partition_ids = Vec::new();
        let mut max_id = 0u32;

        for part in device.partitions() {
            let mut region = Region::new(part.start, part.end);
            region.partition_id = Some(part.number);
            original_regions.push(region);
            original_partition_ids.push(part.number);
            max_id = max_id.max(part.number);
        }

        Self {
            usable_start: 0,
            usable_end: device.size(),
            changes: VecDeque::new(),
            original_regions,
            original_partition_ids,
            next_partition_id: max_id + 1,
            wipe_disk: false,
        }
    }

    /// Set the usable disk region offsets
    pub fn with_start_offset(self, offset: u64) -> Self {
        Self {
            usable_start: offset,
            ..self
        }
    }

    /// Set the usable disk region offsets
    pub fn with_end_offset(self, offset: u64) -> Self {
        Self {
            usable_end: offset,
            ..self
        }
    }

    /// Get a human readable description of pending changes
    pub fn describe_changes(&self) -> String {
        if self.changes.is_empty() {
            return "No pending changes".to_string();
        }

        let mut description = "Pending changes:\n".to_string();

        for (i, change) in self.changes.iter().enumerate() {
            description.push_str(&format!("  {}: {}\n", i + 1, change.describe(self.usable_size())));
        }

        description
    }

    /// Returns the current effective layout after all pending changes
    pub fn current_layout(&self) -> Vec<Region> {
        let mut layout = self.original_regions.clone();
        let mut deleted_indices = Vec::new();

        // First pass: collect indices to delete
        for change in &self.changes {
            if let Change::DeletePartition {
                original_index,
                partition_id: _,
            } = change
            {
                deleted_indices.push(*original_index);
            }
        }
        // Sort in reverse order to remove from highest index first
        deleted_indices.sort_unstable_by(|a, b| b.cmp(a));

        // Remove deleted partitions
        for index in deleted_indices {
            layout.remove(index);
        }

        // Second pass: add new partitions
        for change in &self.changes {
            if let Change::AddPartition {
                start,
                end,
                partition_id,
                attributes,
            } = change
            {
                debug!("Adding partition {start}..{end} (ID: {partition_id})");
                layout.push(Region {
                    start: *start,
                    end: *end,
                    partition_id: Some(*partition_id),
                    attributes: attributes.clone(),
                });
            }
        }

        debug!("Current layout has {} partitions", layout.len());
        layout
    }

    pub fn plan_add_partition(&mut self, start: u64, end: u64) -> Result<(), PlanError> {
        self.plan_add_partition_with_attributes(start, end, None)
    }

    /// Plan to add a new partition between two absolute positions on disk.
    ///
    /// # Arguments
    /// * `start` - The absolute starting position in bytes from the beginning of the disk
    /// * `end` - The absolute ending position in bytes from the beginning of the disk
    ///
    /// Both positions will be aligned to the nearest appropriate boundary (usually 1MB).
    /// The partition will occupy the range [start, end).
    ///
    pub fn plan_add_partition_with_attributes(
        &mut self,
        start: u64,
        end: u64,
        attributes: Option<PartitionAttributes>,
    ) -> Result<(), PlanError> {
        debug!("Planning to add partition {start}..{end}");
        debug!("Original size requested: {}", end - start);

        // Align start and end positions, capping to usable bounds
        let aligned_start = std::cmp::max(align_up(start, PARTITION_ALIGNMENT), self.usable_start);
        let aligned_end = std::cmp::min(align_down(end, PARTITION_ALIGNMENT), self.usable_end);

        debug!("Aligned positions: {aligned_start}..{aligned_end}");
        debug!("Size after alignment: {}", aligned_end - aligned_start);

        // Validate input alignments
        if is_aligned(start, PARTITION_ALIGNMENT) && aligned_start != start {
            warn!("Start position was already aligned but was re-aligned differently");
            return Err(PlanError::RegionOutOfBounds {
                start: aligned_start,
                end: aligned_end,
            });
        }
        if is_aligned(end, PARTITION_ALIGNMENT) && aligned_end != end {
            warn!("End position was already aligned but was re-aligned differently");
            return Err(PlanError::RegionOutOfBounds {
                start: aligned_start,
                end: aligned_end,
            });
        }
        // Validate bounds against usable disk region
        if aligned_start < self.usable_start || aligned_end > self.usable_end {
            warn!("Partition would be outside usable disk region");
            return Err(PlanError::RegionOutOfBounds {
                start: aligned_start,
                end: aligned_end,
            });
        }

        // Ensure we haven't created a zero-sized partition through alignment
        if aligned_end <= aligned_start {
            warn!("Partition would have zero or negative size after alignment");
            return Err(PlanError::RegionOutOfBounds {
                start: aligned_start,
                end: aligned_end,
            });
        }

        // Check for overlaps with current layout
        let new_region = Region::new(aligned_start, aligned_end);
        let current = self.current_layout();
        for region in &current {
            if new_region.overlaps_with(region) {
                warn!(
                    "Partition would overlap with existing partition at {}..{} - attempted region {}..{}",
                    region.start, region.end, new_region.start, new_region.end
                );
                return Err(PlanError::RegionOverlap {
                    start: aligned_start,
                    end: aligned_end,
                });
            }
        }

        let partition_id = self.allocate_partition_id();
        debug!("Adding new partition with ID {partition_id} to change queue");
        self.changes.push_back(Change::AddPartition {
            start: aligned_start,
            end: aligned_end,
            partition_id,
            attributes,
        });
        Ok(())
    }

    /// Plan to delete an existing partition
    pub fn plan_delete_partition(&mut self, index: usize) -> Result<(), PlanError> {
        debug!("Planning to delete partition at index {index}");

        if index >= self.original_regions.len() {
            warn!("Invalid partition index {index}");
            return Err(PlanError::RegionOutOfBounds {
                start: self.usable_start,
                end: self.usable_size(),
            });
        }

        let partition_id = self
            .get_original_partition_id(index)
            .ok_or(PlanError::RegionOutOfBounds {
                start: self.usable_start,
                end: self.usable_size(),
            })?;

        debug!("Adding deletion of partition ID {partition_id} to change queue");
        self.changes.push_back(Change::DeletePartition {
            original_index: index,
            partition_id,
        });
        Ok(())
    }

    /// Undo the most recent change
    pub fn undo(&mut self) -> bool {
        if let Some(change) = self.changes.pop_back() {
            debug!("Undoing last change: {change:?}");
            true
        } else {
            debug!("No changes to undo");
            false
        }
    }

    /// Clear all planned changes
    pub fn reset(&mut self) {
        eprintln!("Resetting all planned changes");
        self.changes.clear();
    }

    /// Check if there are any pending changes
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
    /// Get the list of pending changes
    pub fn changes(&self) -> &VecDeque<Change> {
        &self.changes
    }

    /// Get the size of the usable disk region in bytes
    pub fn usable_size(&self) -> u64 {
        self.usable_end - self.usable_start
    }

    /// Get the usable disk region offsets
    pub fn offsets(&self) -> (u64, u64) {
        (self.usable_start, self.usable_end)
    }

    /// Plan to initialize a clean partition layout
    pub fn plan_initialize_disk(&mut self) -> Result<(), PlanError> {
        debug!("Planning to create new GPT partition table");
        self.changes.clear(); // Clear any existing changes
        self.original_regions.clear(); // Clear original partitions
        self.original_partition_ids.clear();
        self.next_partition_id = 1;
        self.wipe_disk = true;
        Ok(())
    }

    pub fn wipe_disk(&self) -> bool {
        self.wipe_disk
    }
    /// Get the next available partition ID and increment the counter
    pub fn allocate_partition_id(&mut self) -> u32 {
        let id = self.next_partition_id;
        self.next_partition_id += 1;
        id
    }

    /// Get the original partition ID for a given index
    pub fn get_original_partition_id(&self, index: usize) -> Option<u32> {
        self.original_partition_ids.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use disks::mock::MockDisk;
    use test_log::test;

    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;

    /// Creates a mock disk with a typical size of 500GB
    fn create_mock_disk() -> MockDisk {
        MockDisk::new(500 * GB)
    }

    /// Creates a mock disk with an existing Windows installation
    /// Layout:
    /// - EFI System Partition (ESP): 100MB
    /// - Microsoft Reserved: 16MB
    /// - Windows C: Drive: 200GB
    /// - Recovery: 500MB
    fn create_windows_disk() -> MockDisk {
        let mut disk = MockDisk::new(500 * GB);
        // All positions are absolute start/end, not sizes
        disk.add_partition(0, 100 * MB); // ESP: 0 -> 100MB
        disk.add_partition(100 * MB, 116 * MB); // MSR: 100MB -> 116MB
        disk.add_partition(116 * MB, 200 * GB + 116 * MB); // Windows: 116MB -> 200.116GB
        disk.add_partition(200 * GB + 116 * MB, 200 * GB + 616 * MB); // Recovery: 200.116GB -> 200.616GB
        disk
    }

    #[test]
    fn test_fresh_installation() {
        let disk = create_mock_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Create typical Linux partition layout with absolute positions
        // - 0 -> 512MB: EFI System Partition
        // - 512MB -> 4.5GB: Swap
        // - 4.5GB -> 500GB: Root
        assert!(planner.plan_add_partition(0, 512 * MB).is_ok());
        assert!(planner.plan_add_partition(512 * MB, 4 * GB + 512 * MB).is_ok());
        assert!(planner.plan_add_partition(4 * GB + 512 * MB, 500 * GB).is_ok());

        eprintln!("\nPlanned fresh installation:");
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 3);
        assert_eq!(layout[0].size(), 512 * MB);
        assert_eq!(layout[1].size(), 4 * GB);
    }

    #[test]
    fn test_dual_boot_with_windows() {
        let disk = create_windows_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Available space starts after Windows partitions (~ 200.6GB)
        let start = 200 * GB + 616 * MB;

        // Create Linux partitions in remaining space
        // - 4GB swap
        // - Rest for root
        assert!(planner.plan_add_partition(start, start + 4 * GB).is_ok());
        assert!(planner.plan_add_partition(start + 4 * GB, 500 * GB).is_ok());

        eprintln!("\nPlanned dual-boot changes:");
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 6); // 4 Windows + 2 Linux partitions
    }

    #[test]
    fn test_replace_linux() {
        let mut disk = create_mock_disk();
        // Simulate existing Linux installation
        // All positions are absolute start/end
        disk.add_partition(0, 512 * MB); // ESP: 0 -> 512MB
        disk.add_partition(512 * MB, 4 * GB + 512 * MB); // Swap: 512MB -> 4.5GB
        disk.add_partition(4 * GB + 512 * MB, 500 * GB); // Root: 4.5GB -> 500GB

        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Delete old Linux partitions
        assert!(planner.plan_delete_partition(1).is_ok()); // Delete swap
        assert!(planner.plan_delete_partition(2).is_ok()); // Delete root

        // Create new layout (keeping ESP)
        // - 8GB swap (larger than before)
        // - Rest for root
        assert!(planner.plan_add_partition(512 * MB, 8 * GB + 512 * MB).is_ok());
        assert!(planner.plan_add_partition(8 * GB + 512 * MB, 500 * GB).is_ok());

        eprintln!("\nPlanned Linux replacement changes:");
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 3);
        assert_eq!(layout[1].size(), 8 * GB);
    }

    #[test]
    fn test_region_validation() {
        let disk = create_mock_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Test out of bounds
        assert!(matches!(
            planner.plan_add_partition(0, 600 * GB),
            Err(PlanError::RegionOutOfBounds { .. })
        ));

        // Add a partition and test overlap
        assert!(planner.plan_add_partition(0, 100 * GB).is_ok());
        assert!(matches!(
            planner.plan_add_partition(50 * GB, 150 * GB),
            Err(PlanError::RegionOverlap { .. })
        ));
    }

    #[test]
    fn test_undo_operations() {
        let disk = create_mock_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Add some partitions
        assert!(planner.plan_add_partition(0, 100 * GB).is_ok());
        assert!(planner.plan_add_partition(100 * GB, 200 * GB).is_ok());
        assert_eq!(planner.current_layout().len(), 2);

        // Undo last addition
        assert!(planner.undo());
        assert_eq!(planner.current_layout().len(), 1);

        // Undo first addition
        assert!(planner.undo());
        assert_eq!(planner.current_layout().len(), 0);

        // Verify no more changes to undo
        assert!(!planner.undo());
    }

    #[test]
    fn test_partition_boundaries() {
        let disk = create_mock_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Add first partition from 0 to 100GB
        assert!(planner.plan_add_partition(0, 100 * GB).is_ok());

        // Next partition should be able to start exactly where previous one ended
        assert!(planner.plan_add_partition(100 * GB, 200 * GB).is_ok());

        // Verify partitions are properly adjacent
        let layout = planner.current_layout();
        assert_eq!(layout.len(), 2);
        assert_eq!(layout[0].end, layout[1].start);

        // Attempting to create partition overlapping either boundary should fail
        assert!(matches!(
            planner.plan_add_partition(99 * GB, 150 * GB),
            Err(PlanError::RegionOverlap { .. })
        ));
        assert!(matches!(
            planner.plan_add_partition(150 * GB, 201 * GB),
            Err(PlanError::RegionOverlap { .. })
        ));
    }

    #[test]
    fn test_alignment() {
        let disk = create_mock_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Already aligned values should not be re-aligned
        let aligned_start = PARTITION_ALIGNMENT;
        let aligned_end = 2 * PARTITION_ALIGNMENT;
        assert!(planner.plan_add_partition(aligned_start, aligned_end).is_ok());

        // Test that non-aligned values get properly aligned
        let unaligned_start = (2 * PARTITION_ALIGNMENT) + 100;
        let unaligned_end = (3 * PARTITION_ALIGNMENT) - 100;
        assert!(planner.plan_add_partition(unaligned_start, unaligned_end).is_ok());

        let layout = planner.current_layout();
        assert_eq!(layout[0].start, aligned_start);
        assert_eq!(layout[0].end, aligned_end);

        assert_eq!(layout[1].start, 2 * PARTITION_ALIGNMENT); // Aligned up
        assert_eq!(layout[1].end, 3 * PARTITION_ALIGNMENT); // Aligned down
    }

    #[test]
    fn test_alignment_functions() {
        let mb = 1024 * 1024;
        let kb = 1024;

        // Test align_up
        assert_eq!(align_up(2 * mb + 100, mb), 2 * mb);
        assert_eq!(align_up(2 * mb, mb), 2 * mb); // Already aligned

        // Test align_up past boundary
        assert_eq!(align_up(2 * mb + (600 * kb), mb), 3 * mb);

        // Test align_down
        assert_eq!(align_down(4 * mb - 100, mb), 4 * mb);
        assert_eq!(align_down(4 * mb, mb), 4 * mb); // Already aligned

        // Test align_down past boundary

        assert_eq!(align_down(4 * mb + (600 * kb), mb), 5 * mb);
    }

    #[test]
    fn test_initialize_disk_partition_numbers() {
        let mut disk = create_mock_disk();
        // Add some existing partitions
        disk.add_partition(0, 100 * MB);
        disk.add_partition(100 * MB, 200 * MB);
        disk.add_partition(200 * MB, 300 * MB);

        let mut planner = Planner::new(&BlockDevice::mock_device(disk));

        // Initialize disk should reset partition numbering
        assert!(planner.plan_initialize_disk().is_ok());

        // Add new partitions - should start from 1
        assert!(planner.plan_add_partition(0, 100 * MB).is_ok());
        assert!(planner.plan_add_partition(100 * MB, 200 * MB).is_ok());

        let layout = planner.current_layout();
        assert_eq!(layout[0].partition_id, Some(1));
        assert_eq!(layout[1].partition_id, Some(2));
    }
}
