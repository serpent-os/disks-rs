// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

use crate::{Context, get_property_str};
use crate::{FromKdlProperty, PartitionTableType, get_kdl_property};

/// Command to create a partition table
#[derive(Debug)]
pub struct Command {
    /// The type of partition table to create
    pub table_type: PartitionTableType,
    pub disk: String,
}

/// Generate a command to create a partition table
pub(crate) fn parse(context: Context<'_>) -> Result<super::Command, crate::Error> {
    let kind = get_kdl_property(context.node, "type")?;
    let table_type = PartitionTableType::from_kdl_property(kind)?;
    let disk = get_property_str(context.node, "disk")?;

    Ok(super::Command::CreatePartitionTable(Box::new(Command {
        table_type,
        disk,
    })))
}
