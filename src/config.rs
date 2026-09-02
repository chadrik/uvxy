//! Find the mappings, and read them.
//!
//! See `docs/adr/0001-separate-config-file-no-directory-search.md`.

use crate::Mappings;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The mappings, and where each one came from.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Every command, mapped to a package spec.
    pub mappings: Mappings,
    /// Every command, mapped to the file that supplied it.
    pub sources: BTreeMap<String, PathBuf>,
    /// Every file that `uvxy` read, in the order it read them.
    pub files_read: Vec<PathBuf>,
}

/// Read every configuration file, and merge the results.
///
/// `UVXY_CONFIG_FILE` replaces every other file. uv gives `UV_CONFIG_FILE` the
/// same meaning. Otherwise the user file merges over the system file, one
/// command name at a time, and the user file wins.
///
/// Return an empty `Config` when no file exists. Return an error when a file
/// exists and does not parse.
pub fn load() -> anyhow::Result<Config> {
    todo!("phase 1: src/config.rs")
}

/// Return the system path, then the user path. A later path wins.
///
/// Unix, and macOS: `/etc/uv/uvxy.toml`, then `$XDG_CONFIG_HOME/uv/uvxy.toml`.
/// Read `$HOME/.config` when `XDG_CONFIG_HOME` is empty. uv reads
/// `XDG_CONFIG_HOME` on macOS, so `uvxy` reads it there too. Do not call a
/// crate that returns `~/Library/Application Support` on macOS.
///
/// Windows: `%APPDATA%\uv\uvxy.toml`.
pub fn config_paths() -> Vec<PathBuf> {
    todo!("phase 1: src/config.rs")
}

/// Read the `[from]` table out of one file's text.
///
/// Each key is a command. Each value is a package spec string. Match a key
/// exactly, and never normalize it. Return an error when the text does not
/// parse, when `[from]` is not a table, or when a value is not a string.
pub fn parse(text: &str) -> anyhow::Result<Mappings> {
    todo!("phase 1: src/config.rs")
}
