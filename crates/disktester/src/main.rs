// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};

use disks::BlockDevice;
use partitioning::{Formatter, blkpg, loopback, sparsefile, writer::DiskWriter};
use provisioning::{Parser, Provisioner, StrategyDefinition};

/// Loads provisioning strategies from a configuration file
///
/// # Arguments
/// * `path` - Path to the provisioning configuration file
///
/// # Returns
/// * `Result<Vec<StrategyDefinition>>` - Vector of loaded strategy definitions
fn load_provisioning(path: impl Into<PathBuf>) -> Result<Vec<StrategyDefinition>, Box<dyn std::error::Error>> {
    let p = path.into();
    let parser = Parser::new_for_path(p)?;
    Ok(parser.strategies)
}

/// Applies partitioning strategies to a block device
///
/// # Arguments
/// * `whence` - Path to the block device to partition
///
/// # Returns
/// * `Result<()>` - Success or error status
fn apply_partitioning(whence: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize provisioner and load strategies
    let mut prov = Provisioner::new();
    let strategies = load_provisioning("crates/provisioning/tests/use_whole_disk.kdl")?;
    for strategy in &strategies {
        prov.add_strategy(strategy);
    }

    // Set up block device
    let device = disks::loopback::Device::from_device_path(whence).ok_or("Not a loop device")?;
    let blk = BlockDevice::loopback_device(device);
    prov.push_device(&blk);

    // Generate and validate partitioning plans
    let plans = prov.plan();
    for plan in &plans {
        eprintln!("Plan: {}", plan.strategy.name);
    }
    let plan = plans.first().ok_or("No plans")?;

    // Apply partitioning changes
    for (disk, device_plan) in plan.device_assignments.iter() {
        eprintln!("strategy for {} is now: {}", disk, device_plan.strategy.describe());
        eprintln!("After: {}", device_plan.planner.describe_changes());

        let disk_writer = DiskWriter::new(device_plan.device, &device_plan.planner);
        disk_writer.simulate()?;
        eprintln!("Simulation passed");
        disk_writer.write()?;
    }

    // Sync partition table changes
    blkpg::sync_gpt_partitions(whence)?;

    let mut formatters = plan
        .filesystems
        .iter()
        .map(|(device, fs)| {
            let formatter = Formatter::new(fs.clone()).force();
            formatter.format(device)
        })
        .collect::<Vec<_>>();

    for operation in formatters.iter_mut() {
        match operation.output() {
            Ok(output) => {
                let stdout = std::str::from_utf8(&output.stdout).expect("Invalid UTF-8");
                if output.status.success() {
                    eprintln!("Format success: {stdout}");
                } else {
                    let stderr = std::str::from_utf8(&output.stderr).expect("Invalid UTF-8");
                    eprintln!("Format error: {stderr}");
                }
                eprintln!("Format output: {stdout}");
            }
            Err(e) => {
                eprintln!("Format error: {e}");
            }
        }
    }

    eprintln!("Format operations: {formatters:?}");

    for (role, device) in plan.role_mounts.iter() {
        eprintln!("To mount: {:?} as {:?} (`{}`)", device, role, role.as_path());
    }

    Ok(())
}

/// Main entry point - creates and partitions a loopback device
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create sparse file and attach loopback device
    sparsefile::create("lesparse.img", 32 * 1024 * 1024 * 1024)?;
    let l = loopback::LoopDevice::create()?;
    l.attach("lesparse.img")?;

    eprintln!("Loopback device: {:?}", &l.path);

    // Apply partitioning and handle errors
    let whence = PathBuf::from(&l.path);
    if let Err(e) = apply_partitioning(&whence) {
        eprintln!("Error applying partitioning: {e}");
    }

    // Clean up loopback device
    l.detach()?;

    Ok(())
}
