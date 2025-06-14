// SPDX-FileCopyrightText: Copyright © 2025 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

use itertools::Itertools;

use crate::{Constraints, Context};

#[derive(Debug)]
pub struct Command {
    pub name: String,
    pub constraints: Option<Constraints>,
}

/// Generate a command to find a disk
pub(crate) fn parse(context: Context<'_>) -> Result<super::Command, crate::Error> {
    let arguments = context
        .node
        .entries()
        .iter()
        .filter(|e| e.is_empty() || e.name().is_none())
        .collect_vec();

    let name = match arguments.len() {
        0 => {
            return Err(crate::InvalidArguments {
                at: context.node.span(),
                advice: Some("find-disk <name> - you must provide a name to store the object".into()),
            }
            .into());
        }
        1 => arguments[0].value().as_string().ok_or(crate::InvalidType {
            at: arguments[0].span(),
            expected_type: crate::KdlType::String,
        })?,
        _ => {
            return Err(crate::InvalidArguments {
                at: context.node.span(),
                advice: Some("find-disk <name> - only one positional argument supported".into()),
            }
            .into());
        }
    };

    let constraints =
        if let Some(constraints) = context.node.iter_children().find(|n| n.name().value() == "constraints") {
            Some(Constraints::from_kdl_node(constraints)?)
        } else {
            None
        };

    Ok(super::Command::FindDisk(Box::new(Command {
        name: name.to_owned(),
        constraints,
    })))
}
