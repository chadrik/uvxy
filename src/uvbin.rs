//! Find the uv binary, and hand the arguments to it.

use std::path::PathBuf;

/// Which binary `uvxy` found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvKind {
    /// The `uv` binary. `uvxy` runs `uv tool run`.
    Uv,
    /// The `uvx` binary. `uvxy` found no `uv`, so it cannot read the flags.
    Uvx,
}

/// The binary that `uvxy` runs.
#[derive(Debug, Clone)]
pub struct Uv {
    pub path: PathBuf,
    pub kind: UvKind,
}

impl Uv {
    /// Return the arguments that come before the user's arguments.
    pub fn prefix_args(&self) -> &'static [&'static str] {
        match self.kind {
            UvKind::Uv => &["tool", "run"],
            UvKind::Uvx => &[],
        }
    }
}

/// Find the uv binary.
///
/// Read `$UVXY_UV` first. Then read `$UV`, which uv sets for its own child
/// processes. Then search `PATH` for `uv`.
///
/// Fall back to `uvx` when no `uv` exists. `uvx` cannot produce completions,
/// so the caller then reads the first argument as the command.
///
/// Return an error when neither binary exists.
pub fn resolve() -> anyhow::Result<Uv> {
    todo!("phase 1: src/uvbin.rs")
}

/// Hand the arguments to uv.
///
/// Replace the current process on Unix, with `execvp`. The signals, the
/// terminal, and the exit code then belong to uv, and `uvxy` adds no layer.
/// This call does not return when it succeeds.
///
/// Windows has no `exec`. Start a child process there, wait for it, and return
/// its exit code.
pub fn exec(uv: &Uv, args: &[String]) -> anyhow::Result<i32> {
    todo!("phase 1: src/uvbin.rs")
}
