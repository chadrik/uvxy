//! Find the command, and build the arguments for uv.
//!
//! This module reads no file and starts no process. The tests target it.
//!
//! See `docs/adr/0003-drop-in-replacement-and-flag-namespace.md`.

use crate::Mappings;
use crate::flags::FlagTable;

/// The mapping that `uvxy` used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The command that the user typed, without any `@version` part.
    pub command: String,
    /// The value that `uvxy` gave to `--from`.
    pub spec: String,
}

/// What `uvxy` will do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// The arguments for `uv tool run`. This list excludes `tool` and `run`.
    pub uv_args: Vec<String>,
    /// The `--uvxy-` flags that `uvxy` removed.
    pub uvxy_flags: Vec<String>,
    /// The command, when `uvxy` found one.
    pub command: Option<String>,
    /// The mapping, when `uvxy` applied one.
    pub applied: Option<Applied>,
}

/// Build the arguments for uv.
///
/// `table` holds the uv flags that take a value. A `None` table means that
/// `uvxy` could not read the flags. Read the first argument as the command in
/// that case.
///
/// The rules:
///
/// 1. Read the arguments from left to right.
/// 2. Remove an argument that starts with `--uvxy-`. Record it in
///    `uvxy_flags`. Never send it to uv.
/// 3. Skip a flag. Skip the argument after it when the table says that the
///    flag takes a value. A `--flag=value` argument carries its own value.
/// 4. Stop at `--`. The argument after `--` is the command.
/// 5. The first argument that is neither a flag nor a flag value is the
///    command.
/// 6. Copy every argument after the command without a change.
/// 7. Insert `--from <spec>` directly before the command. Insert it before a
///    `--` separator when one is present.
/// 8. Insert nothing when the user already passed `--from`.
/// 9. Split a command that reads `name@version`. Look up `name`. Build
///    `--from <spec>@<version>`, and pass `name` as the command.
/// 10. Return an error when the spec already holds a version and the user also
///     passed `@version`.
pub fn rewrite(
    args: &[String],
    table: Option<&FlagTable>,
    mappings: &Mappings,
) -> anyhow::Result<Plan> {
    todo!("phase 1: src/rewrite.rs")
}
