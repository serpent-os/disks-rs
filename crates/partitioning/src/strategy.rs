// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0
//! High-level partition allocation strategies
//!
//! This module provides an abstraction layer for common disk partitioning patterns.
//! Rather than manually planning individual partition changes, consumers can:
//!
//! 1. Choose a high-level strategy (e.g. initialize whole disk, use largest free space)
//! 2. Define their partition requirements (exact sizes, minimums, ranges)
//! 3. Let the strategy handle the details of planning the actual changes
//!
//! Example:
//! ```no_run
//! use partitioning::strategy::{Strategy, AllocationStrategy, PartitionRequest, SizeRequirement};
//!
//! // Create strategy for fresh installation
//! let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);
//!
//! // Request needed partitions
//! strategy.add_request(PartitionRequest {
//!     size: SizeRequirement::Exact(512 * 1024 * 1024), // 512MB EFI partition
//!     attributes: None,
//! });
//! strategy.add_request(PartitionRequest {
//!     size: SizeRequirement::Remaining, // Rest for root
//!     attributes: None,
//! });
//! ```

use crate::planner::{PlanError, Planner};

use crate::PartitionAttributes;
use crate::planner::Region;

/// Strategy for allocating partitions
#[derive(Debug, Clone)]
pub enum AllocationStrategy {
    /// Initialize a clean partition layout using the entire disk.
    /// This will remove all existing partitions and create a new layout.
    InitializeWholeDisk,
    /// Use largest available free region on existing table
    LargestFree,
    /// Use first free region that fits on existing table
    FirstFit,
    /// Use specific region on existing table
    SpecificRegion(Region),
}

/// Defines how to size a partition within its allocated region
#[derive(Debug, Clone)]
pub enum SizeRequirement {
    /// Exact size in bytes
    Exact(u64),
    /// Minimum size in bytes, using more if available
    AtLeast(u64),
    /// Between min and max bytes
    Range { min: u64, max: u64 },
    /// Use all remaining space
    Remaining,
}

/// A partition request for the strategy to plan
#[derive(Debug, Clone)]
pub struct PartitionRequest {
    pub size: SizeRequirement,
    pub attributes: Option<PartitionAttributes>,
}

/// Handles planning partition layouts according to specific strategies
#[derive(Debug, Clone)]
pub struct Strategy {
    allocation: AllocationStrategy,
    requests: Vec<PartitionRequest>,
}

impl Strategy {
    /// Create a new strategy using the specified allocation method
    pub fn new(allocation: AllocationStrategy) -> Self {
        Self {
            allocation,
            requests: Vec::new(),
        }
    }

    /// Add a partition request to this strategy
    pub fn add_request(&mut self, request: PartitionRequest) {
        self.requests.push(request);
    }

    /// Find available free regions on the disk
    fn find_free_regions(&self, planner: &Planner) -> Vec<Region> {
        let mut regions = Vec::new();
        let (mut current, disk_size) = planner.offsets();

        // Sort existing partitions by start position
        let mut layout = planner.current_layout();
        layout.sort_by_key(|r| r.start);

        // Find gaps between partitions
        for region in layout {
            if region.start > current {
                regions.push(Region::new(current, region.start));
            }
            current = region.end;
        }

        // Add final region if there's space after last partition
        if current < disk_size {
            regions.push(Region::new(current, disk_size));
        }

        regions
    }

    /// Get a human readable description of this strategy
    pub fn describe(&self) -> String {
        use disks::format_size;

        let mut desc = match &self.allocation {
            AllocationStrategy::InitializeWholeDisk => "Initialize new partition layout on entire disk".to_string(),
            AllocationStrategy::LargestFree => "Use largest free region".to_string(),
            AllocationStrategy::FirstFit => "Use first available region".to_string(),
            AllocationStrategy::SpecificRegion(r) => format!("Use specific region: {}", r.describe(r.end - r.start)),
        };

        if !self.requests.is_empty() {
            desc.push_str("\nRequested partitions:\n");
            for (i, req) in self.requests.iter().enumerate() {
                let size_desc = match &req.size {
                    SizeRequirement::Exact(size) => format!("exactly {}", format_size(*size)),
                    SizeRequirement::AtLeast(min) => format!("at least {}", format_size(*min)),
                    SizeRequirement::Range { min, max } => {
                        format!("between {} and {}", format_size(*min), format_size(*max))
                    }
                    SizeRequirement::Remaining => "remaining space".to_string(),
                };
                desc.push_str(&format!("  {}: {}\n", i + 1, size_desc));
            }
        }
        desc
    }

    /// Apply this strategy to a planner
    /// This will plan the necessary partition changes to fulfill the requirements
    /// Returns an error if the strategy cannot be applied due to insufficient space
    /// or other constraints
    pub fn apply(&self, planner: &mut Planner) -> Result<(), PlanError> {
        // Determine the target region for our partitions
        let target = match &self.allocation {
            AllocationStrategy::InitializeWholeDisk => {
                // Clear existing partitions and start fresh
                planner.plan_initialize_disk()?;
                let (start, end) = planner.offsets();
                Region::new(start, end)
            }
            AllocationStrategy::LargestFree => {
                let free_regions = self.find_free_regions(planner);
                free_regions
                    .iter()
                    .max_by_key(|r| r.size())
                    .cloned()
                    .ok_or(PlanError::NoFreeRegions)?
            }
            AllocationStrategy::FirstFit => {
                let free_regions = self.find_free_regions(planner);
                free_regions.first().cloned().ok_or(PlanError::NoFreeRegions)?
            }
            AllocationStrategy::SpecificRegion(region) => region.clone(),
        };

        let mut current = target.start;
        let mut remaining = target.end - target.start;

        let mut flexible_requests = Vec::new();
        let mut total_fixed = 0u64;
        let mut min_flexible = 0u64;

        // First pass: Calculate space requirements
        for (current_idx, request) in self.requests.iter().enumerate() {
            match &request.size {
                SizeRequirement::Exact(size) => total_fixed += size,
                SizeRequirement::AtLeast(min) => {
                    min_flexible += min;
                    flexible_requests.push((current_idx, *min, None));
                }
                SizeRequirement::Range { min, max } => {
                    min_flexible += min;
                    flexible_requests.push((current_idx, *min, Some(*max)));
                }
                SizeRequirement::Remaining => {
                    flexible_requests.push((current_idx, 0, None));
                }
            }
        }

        // Verify we have enough space for minimum requirements
        if total_fixed + min_flexible > remaining {
            return Err(PlanError::RegionOutOfBounds {
                start: current,
                end: current + total_fixed + min_flexible,
            });
        }

        // First pass: allocate exact size partitions
        for request in &self.requests {
            if let SizeRequirement::Exact(size) = request.size {
                planner.plan_add_partition_with_attributes(current, current + size, request.attributes.clone())?;
                current += size;
                remaining -= size;
            }
        }

        // Second pass: allocate flexible partitions
        let mut remaining_flexible = flexible_requests.len();
        for (idx, min, max_opt) in &flexible_requests {
            remaining_flexible -= 1;

            // First verify we have enough space for minimum requirement
            if *min > remaining {
                // Clean up any changes we made since we can't complete all requests
                while planner.has_changes() {
                    planner.undo();
                }
                return Err(PlanError::RegionOutOfBounds {
                    start: current,
                    end: current + min,
                });
            }

            let size = if remaining_flexible == 0 {
                // Last flexible partition gets all remaining space
                let size = remaining;
                if let Some(max) = max_opt {
                    size.min(*max).max(*min)
                } else {
                    size.max(*min)
                }
            } else {
                // Other flexible partitions get fair share plus minimum
                let share = remaining / (remaining_flexible + 1) as u64;
                let size = min + share;
                if let Some(max) = max_opt { size.min(*max) } else { size }
            };

            match planner.plan_add_partition_with_attributes(
                current,
                current + size,
                self.requests.get(*idx).and_then(|r| r.attributes.clone()),
            ) {
                Ok(_) => {
                    current += size;
                    remaining -= size;
                }
                Err(e) => {
                    // Clean up any changes we made since we can't complete all requests
                    while planner.has_changes() {
                        planner.undo();
                    }
                    return Err(e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::Planner;
    use disks::{BlockDevice, mock::MockDisk};
    use test_log::test;

    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;

    // Common partition sizes for Linux installations
    const EFI_SIZE: u64 = 512 * MB; // Standard EFI partition size
    const BOOT_SIZE: u64 = GB; // /boot partition size
    const SWAP_MIN: u64 = 4 * GB; // Minimum swap size
    const SWAP_MAX: u64 = 8 * GB; // Maximum swap size
    const ROOT_MIN: u64 = 20 * GB; // Minimum root partition size
    const ROOT_MAX: u64 = 100 * GB; // Maximum root partition size

    /// Creates a root partition request that uses remaining space with a minimum size
    fn root_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::AtLeast(ROOT_MIN),
            attributes: None,
        }
    }

    /// Creates a root partition request capped at 100GB, suitable for layouts with home partition
    fn capped_root_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::Range {
                min: ROOT_MIN,
                max: ROOT_MAX,
            },
            attributes: None,
        }
    }

    /// Creates a standard EFI system partition request
    fn efi_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::Exact(EFI_SIZE),
            attributes: None,
        }
    }

    /// Creates a /boot partition request
    fn boot_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::Exact(BOOT_SIZE),
            attributes: None,
        }
    }

    /// Creates a swap partition request that scales with system RAM
    fn swap_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::Range {
                min: SWAP_MIN,
                max: SWAP_MAX,
            },
            attributes: None,
        }
    }

    /// Creates a home partition request that uses all remaining space
    fn home_partition() -> PartitionRequest {
        PartitionRequest {
            size: SizeRequirement::Remaining,
            attributes: None,
        }
    }
    fn create_test_disk() -> MockDisk {
        MockDisk::new(500 * GB)
    }

    #[test]
    fn test_uefi_clean_install() {
        // Test case: Clean UEFI installation with separate /home
        let disk = create_test_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);

        // Standard UEFI layout with separate /home
        strategy.add_request(efi_partition());
        strategy.add_request(boot_partition());
        strategy.add_request(swap_partition());
        strategy.add_request(capped_root_partition());
        strategy.add_request(home_partition());

        eprintln!("\nUEFI Clean Install Strategy:\n{}", strategy.describe());
        assert!(strategy.apply(&mut planner).is_ok());
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 5);
        // Verify partition order and basic size requirements
        assert!(layout[0].size() >= EFI_SIZE);
        assert!(layout[1].size() >= BOOT_SIZE);
        assert!(layout[2].size() >= SWAP_MIN);
        assert!(layout[3].size() >= ROOT_MIN);
    }

    #[test]
    fn test_dual_boot_install() {
        // Test case: Installation alongside existing Windows
        let mut disk = create_test_disk();

        // Simulate existing Windows layout:
        // 100MB EFI + 16MB MSR + 200GB Windows + Free Space
        disk.add_partition(0, 100 * MB); // EFI
        disk.add_partition(100 * MB, 116 * MB); // MSR
        disk.add_partition(116 * MB, 200 * GB); // Windows

        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::LargestFree);

        // Standard Linux layout using remaining space
        strategy.add_request(swap_partition());
        strategy.add_request(root_partition());

        eprintln!("\nDual Boot Strategy:\n{}", strategy.describe());
        assert!(strategy.apply(&mut planner).is_ok());
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 5); // 3 Windows + 2 Linux partitions
    }

    #[test]
    fn test_minimal_server_install() {
        // Test case: Minimal server installation with single root partition
        let disk = create_test_disk();
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);

        // Simple layout - just boot and root
        strategy.add_request(boot_partition());
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::Remaining,
            attributes: None,
        });

        eprintln!("\nMinimal Server Strategy:\n{}", strategy.describe());
        assert!(strategy.apply(&mut planner).is_ok());
        eprintln!("{}", planner.describe_changes());

        let layout = planner.current_layout();
        assert_eq!(layout.len(), 2);
    }

    #[test]
    fn test_insufficient_space() {
        let disk = MockDisk::new(10 * GB); // Intentionally small disk
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);

        // Try to allocate more than available
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::Exact(20 * GB),
            attributes: None,
        });

        assert!(strategy.apply(&mut planner).is_err());
    }

    #[test]
    fn test_flexible_partition_overflow() {
        let disk = MockDisk::new(10 * GB);
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);

        // Request more than available in flexible partitions
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::AtLeast(6 * GB),
            attributes: None,
        });
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::AtLeast(6 * GB),
            attributes: None,
        });

        // Should fail because total minimum (12GB) exceeds disk size (10GB)
        let result = strategy.apply(&mut planner);
        assert!(matches!(result, Err(PlanError::RegionOutOfBounds { .. })));
        assert!(!planner.has_changes());
    }

    #[test]
    fn test_partial_partition_creation() {
        let disk = MockDisk::new(8 * GB);
        let mut planner = Planner::new(&BlockDevice::mock_device(disk));
        let mut strategy = Strategy::new(AllocationStrategy::InitializeWholeDisk);

        // Request sequence where first two would fit but third won't
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::Range { min: GB, max: 2 * GB },
            attributes: None,
        });
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::Range {
                min: 2 * GB,
                max: 4 * GB,
            },
            attributes: None,
        });
        strategy.add_request(PartitionRequest {
            size: SizeRequirement::Range {
                min: 25 * GB,
                max: 120 * GB,
            },
            attributes: None,
        });

        // Should fail and undo partial changes
        let result = strategy.apply(&mut planner);
        assert!(matches!(result, Err(PlanError::RegionOutOfBounds { .. })));
        assert!(!planner.has_changes(), "Partial changes should be undone");
    }
}
