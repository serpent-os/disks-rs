// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
// SPDX-FileCopyrightText: Copyright © 2025 AerynOS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::{fmt, str::FromStr};

pub use gpt::partition_types::Type as GptPartitionType;
pub use uuid::Uuid;

#[cfg(feature = "kdl")]
use crate::{UnsupportedValue, get_kdl_entry, kdl_value_to_string};

#[cfg(feature = "kdl")]
use super::FromKdlType;

#[cfg(feature = "kdl")]
pub enum PartitionTypeKDL {
    GUID,
}

#[cfg(feature = "kdl")]
impl FromStr for PartitionTypeKDL {
    type Err = crate::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "guid" => Ok(Self::GUID),
            _ => Err(crate::Error::UnknownVariant),
        }
    }
}

#[cfg(feature = "kdl")]
impl fmt::Display for PartitionTypeKDL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GUID => f.write_str("guid"),
        }
    }
}

#[cfg(feature = "kdl")]
impl<'a> FromKdlType<'a> for PartitionTypeKDL {
    fn from_kdl_type(id: &'a kdl::KdlEntry) -> Result<Self, crate::Error> {
        let ty_id = if let Some(ty) = id.ty() {
            ty.value().to_lowercase()
        } else {
            "".into()
        };

        let v = ty_id.parse().map_err(|_| UnsupportedValue {
            at: id.span(),
            advice: Some("only '(GUID)' type is supported".into()),
        })?;
        Ok(v)
    }
}

/// Represents GPT partition type GUIDs
#[derive(Debug, PartialEq)]
pub enum PartitionTypeGuid {
    EfiSystemPartition,
    ExtendedBootLoader,
    LinuxSwap,
    LinuxFilesystem,
}

impl fmt::Display for PartitionTypeGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EfiSystemPartition => f.write_str("EFI System Partition"),
            Self::ExtendedBootLoader => f.write_str("Linux Extended Boot"),
            Self::LinuxFilesystem => f.write_str("Linux Filesystem"),
            Self::LinuxSwap => f.write_str("Linux Swap"),
        }
    }
}

impl FromStr for PartitionTypeGuid {
    type Err = crate::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "efi-system-partition" => Ok(Self::EfiSystemPartition),
            "linux-extended-boot" => Ok(Self::ExtendedBootLoader),
            "linux-swap" => Ok(Self::LinuxSwap),
            "linux-fs" => Ok(Self::LinuxFilesystem),
            _ => Err(crate::Error::UnknownVariant),
        }
    }
}

impl PartitionTypeGuid {
    /// Returns the GUID value for this partition type
    pub fn as_guid(&self) -> GptPartitionType {
        match self {
            Self::EfiSystemPartition => gpt::partition_types::EFI,
            Self::ExtendedBootLoader => gpt::partition_types::FREEDESK_BOOT,
            Self::LinuxSwap => gpt::partition_types::LINUX_SWAP,
            Self::LinuxFilesystem => gpt::partition_types::LINUX_FS,
        }
    }

    #[cfg(feature = "kdl")]
    pub fn from_kdl_node(node: &kdl::KdlNode) -> Result<Self, crate::Error> {
        let value = kdl_value_to_string(get_kdl_entry(node, &0)?)?;
        let v = value.parse().map_err(|_| crate::UnsupportedValue {
            at: node.span(),
            advice: Some(
                "'efi-system-partition', 'linux-swap' 'linux-extended-boot' and 'linux-fs' are supported".into(),
            ),
        })?;
        Ok(v)
    }
}
