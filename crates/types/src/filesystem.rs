// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
// SPDX-FileCopyrightText: Copyright © 2025 AerynOS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::{fmt, str::FromStr};

#[cfg(feature = "kdl")]
use crate::{get_kdl_entry, kdl_value_to_integer, kdl_value_to_string};

#[cfg(feature = "kdl")]
use super::FromKdlProperty;

/// The filesystem information for a partition
/// This is used to format the partition with a filesystem
#[derive(Debug, Clone, PartialEq)]
pub enum Filesystem {
    Fat32 {
        label: Option<String>,
        volume_id: Option<u32>,
    },
    Standard {
        filesystem_type: StandardFilesystemType,
        label: Option<String>,
        uuid: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum StandardFilesystemType {
    F2fs,
    Ext4,
    Xfs,
    Swap,
}

impl fmt::Display for StandardFilesystemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ext4 => f.write_str("ext4"),
            Self::F2fs => f.write_str("f2fs"),
            Self::Xfs => f.write_str("xfs"),
            Self::Swap => f.write_str("swap"),
        }
    }
}

impl FromStr for StandardFilesystemType {
    type Err = crate::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ext4" => Ok(Self::Ext4),
            "f2fs" => Ok(Self::F2fs),
            "xfs" => Ok(Self::Xfs),
            "swap" => Ok(Self::Swap),
            _ => Err(crate::Error::UnknownVariant),
        }
    }
}

#[cfg(feature = "kdl")]
impl FromKdlProperty<'_> for StandardFilesystemType {
    fn from_kdl_property(entry: &kdl::KdlEntry) -> Result<Self, crate::Error> {
        let value = kdl_value_to_string(entry)?;
        let v = value.parse().map_err(|_| crate::UnsupportedValue {
            at: entry.span(),
            advice: Some("'fat32', 'ext4', 'f2fs', 'xfs' 'swap' are supported".into()),
        })?;
        Ok(v)
    }
}

#[cfg(feature = "kdl")]
impl Filesystem {
    pub fn from_kdl_node(node: &kdl::KdlNode) -> Result<Self, crate::Error> {
        let mut fs_type = None;
        let mut label = None;
        let mut uuid = None;
        let mut volume_id = None;

        for entry in node.iter_children() {
            match entry.name().value() {
                "type" => fs_type = Some(kdl_value_to_string(get_kdl_entry(entry, &0)?)?),
                "label" => label = Some(kdl_value_to_string(get_kdl_entry(entry, &0)?)?),
                "uuid" => uuid = Some(kdl_value_to_string(get_kdl_entry(entry, &0)?)?),
                "volume_id" => volume_id = Some(kdl_value_to_integer(get_kdl_entry(entry, &0)?)? as u32),
                _ => {
                    return Err(crate::UnsupportedNode {
                        at: entry.span(),
                        name: entry.name().value().into(),
                    }
                    .into());
                }
            }
        }

        let fs_type = fs_type.ok_or(crate::UnsupportedNode {
            at: node.span(),
            name: "type".into(),
        })?;

        match fs_type.as_str() {
            "fat32" => {
                if uuid.is_some() {
                    return Err(crate::InvalidArguments {
                        at: node.span(),
                        advice: Some("FAT32 does not support UUID".into()),
                    }
                    .into());
                }
                Ok(Filesystem::Fat32 { label, volume_id })
            }
            fs_type => {
                if volume_id.is_some() {
                    return Err(crate::InvalidArguments {
                        at: node.span(),
                        advice: Some(format!("volume_id is only supported for FAT32, not {fs_type}")),
                    }
                    .into());
                }
                Ok(Filesystem::Standard {
                    filesystem_type: fs_type.parse()?,
                    label,
                    uuid,
                })
            }
        }
    }
}
