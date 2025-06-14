// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
// SPDX-FileCopyrightText: Copyright © 2025 AerynOS Developers
//
// SPDX-License-Identifier: MPL-2.0

use partitioning::{GptAttributes, PartitionAttributes, TableAttributes, gpt::partition_types};

use crate::{
    Constraints, Context, Filesystem, FromKdlProperty, FromKdlType, PartitionRole, PartitionTypeGuid, PartitionTypeKDL,
    get_kdl_entry, get_kdl_property, get_property_str,
};

/// Command to create a partition
#[derive(Debug)]
pub struct Command {
    /// The disk ID to create the partition on
    pub disk: String,

    /// The reference ID of the partition
    pub id: String,

    /// The role, if any, of the partition
    pub role: Option<PartitionRole>,

    /// The GUID of the partition type
    pub partition_type: Option<PartitionTypeGuid>,

    /// Constraints for the partition
    pub constraints: Constraints,

    /// The filesystem to format the partition with
    pub filesystem: Option<Filesystem>,
}

impl Command {
    pub fn attributes(&self) -> PartitionAttributes {
        PartitionAttributes {
            table: TableAttributes::Gpt(GptAttributes {
                type_guid: match &self.partition_type {
                    Some(p) => p.as_guid(),
                    None => partition_types::BASIC,
                },
                name: self.partition_type.as_ref().map(|p| p.to_string()),
                uuid: None,
            }),
            role: self.role.clone(),
            filesystem: self.filesystem.clone(),
        }
    }
}

/// Generate a command to create a partition
pub(crate) fn parse(context: Context<'_>) -> Result<super::Command, crate::Error> {
    let disk = get_property_str(context.node, "disk")?;
    let id = get_property_str(context.node, "id")?;
    let role = if let Ok(role) = get_kdl_property(context.node, "role") {
        Some(PartitionRole::from_kdl_property(role)?)
    } else {
        None
    };

    let mut constraints = Constraints::default();
    let mut partition_type = None;
    let mut filesystem = None;

    for child in context.node.iter_children() {
        match child.name().value() {
            "constraints" => constraints = Constraints::from_kdl_node(child)?,
            "type" => {
                partition_type = match PartitionTypeKDL::from_kdl_type(get_kdl_entry(child, &0)?)? {
                    PartitionTypeKDL::GUID => Some(PartitionTypeGuid::from_kdl_node(child)?),
                }
            }
            "filesystem" => filesystem = Some(Filesystem::from_kdl_node(child)?),
            _ => {
                return Err(crate::UnsupportedNode {
                    at: child.span(),
                    name: child.name().value().into(),
                }
                .into());
            }
        }
    }

    if matches!(constraints, Constraints::Invalid) {
        return Err(crate::InvalidArguments {
            at: context.node.span(),
            advice: Some("create-partition [disk=<disk>] [role=<role>] [constraints=<constraints>] [type=(GUID)] - you must provide constraints".into()),
        }
        .into());
    }

    Ok(super::Command::CreatePartition(Box::new(Command {
        disk,
        id,
        role,
        constraints,
        partition_type,
        filesystem,
    })))
}
