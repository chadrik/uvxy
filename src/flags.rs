//! Learn which uv flags take a value.
//!
//! See `docs/adr/0002-derive-flag-arity-from-shell-completions.md`.

use crate::uvbin::Uv;
use std::collections::BTreeSet;

/// The `uv tool run` flags that consume the argument after them.
///
/// Names carry their dashes, as in `--from` and `-w`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagTable {
    value_flags: BTreeSet<String>,
}

impl FlagTable {
    /// Build a table. Tests use this.
    pub fn from_flags<I, S>(flags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            value_flags: flags.into_iter().map(Into::into).collect(),
        }
    }

    /// Report whether `flag` consumes the argument after it.
    pub fn takes_value(&self, flag: &str) -> bool {
        self.value_flags.contains(flag)
    }

    /// Report how many flags the table holds.
    pub fn len(&self) -> usize {
        self.value_flags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.value_flags.is_empty()
    }
}

/// Return the flag table for `uv`, from the cache or from uv itself.
pub fn load(uv: &Uv) -> anyhow::Result<FlagTable> {
    todo!("phase 1: src/flags.rs")
}

/// Read the flag table out of `uv generate-shell-completion zsh` output.
///
/// A flag that takes a value carries a `:NAME:` marker, as in
/// `'--from=[Use the given package to provide the command]:FROM:_default'`.
/// A leading `*` marks a repeatable flag, as in `'*--with=[...]:WITH:_default'`.
/// Read only the `uv tool run` section.
///
/// Return an error when the text yields no flags. The caller then warns.
pub fn parse_zsh_completion(text: &str) -> anyhow::Result<FlagTable> {
    todo!("phase 1: src/flags.rs")
}
