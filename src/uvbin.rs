//! Find the uv binary, and hand the arguments to it.

use std::ffi::OsStr;
use std::path::PathBuf;

/// The file name of the uv binary on this platform.
#[cfg(windows)]
const UV_NAME: &str = "uv.exe";
/// The file name of the uv binary on this platform.
#[cfg(not(windows))]
const UV_NAME: &str = "uv";

/// The file name of the uvx binary on this platform.
#[cfg(windows)]
const UVX_NAME: &str = "uvx.exe";
/// The file name of the uvx binary on this platform.
#[cfg(not(windows))]
const UVX_NAME: &str = "uvx";

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
    let uvxy_uv = std::env::var_os("UVXY_UV");
    let uv = std::env::var_os("UV");
    let path = std::env::var_os("PATH");
    lookup(uvxy_uv.as_deref(), uv.as_deref(), path.as_deref())
}

/// Find the uv binary from explicit values.
///
/// `resolve` reads the three values from the environment. The tests supply
/// them directly, because a change to the environment of the process affects
/// every thread.
fn lookup(uvxy_uv: Option<&OsStr>, uv: Option<&OsStr>, path: Option<&OsStr>) -> anyhow::Result<Uv> {
    // `$UVXY_UV` is an explicit choice. A value that names no file is an error,
    // because a silent search of `PATH` hides the mistake.
    if let Some(value) = value_of(uvxy_uv) {
        let candidate = PathBuf::from(value);
        if !candidate.is_file() {
            anyhow::bail!(
                "$UVXY_UV names `{}`, and no file exists at that path",
                candidate.display()
            );
        }
        return Ok(Uv {
            path: candidate,
            kind: UvKind::Uv,
        });
    }

    // uv sets `$UV` to its own path for every child process. A `uvxy` that runs
    // under `uvx` therefore reads the exact binary of the parent.
    if let Some(value) = value_of(uv) {
        let candidate = PathBuf::from(value);
        if candidate.is_file() {
            return Ok(Uv {
                path: candidate,
                kind: UvKind::Uv,
            });
        }
    }

    if let Some(found) = search_path(path, UV_NAME) {
        return Ok(Uv {
            path: found,
            kind: UvKind::Uv,
        });
    }

    // This is the degraded path. `uvx` runs the tool, but it cannot generate
    // shell completions, so the caller gets no flag table.
    if let Some(found) = search_path(path, UVX_NAME) {
        return Ok(Uv {
            path: found,
            kind: UvKind::Uvx,
        });
    }

    anyhow::bail!(
        "no `{UV_NAME}` binary and no `{UVX_NAME}` binary on PATH. \
         Install uv from https://docs.astral.sh/uv/getting-started/installation/, \
         or set $UVXY_UV to the path of the uv binary."
    )
}

/// Return the value of an environment variable, and treat an empty value as an
/// absent value.
fn value_of(value: Option<&OsStr>) -> Option<&OsStr> {
    value.filter(|value| !value.is_empty())
}

/// Search each `PATH` entry for a file with this name.
///
/// Return the first entry that holds an existing file.
fn search_path(path: Option<&OsStr>, name: &str) -> Option<PathBuf> {
    let path = path?;
    for entry in std::env::split_paths(path) {
        // An empty entry means the working directory on some shells. `uvxy`
        // does not read the working directory.
        if entry.as_os_str().is_empty() {
            continue;
        }
        let candidate = entry.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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
    let mut command = std::process::Command::new(&uv.path);
    command.args(uv.prefix_args());
    command.args(args);

    #[cfg(unix)]
    {
        use anyhow::Context;
        use std::os::unix::process::CommandExt;

        // `exec` returns only on failure.
        let error = command.exec();
        Err(error).context(format!("cannot run `{}`", uv.path.display()))
    }

    #[cfg(windows)]
    {
        use anyhow::Context;

        let status = command
            .status()
            .with_context(|| format!("cannot run `{}`", uv.path.display()))?;
        // A signal terminates a process without an exit code on Unix. Windows
        // always reports a code, so the default value is unreachable there.
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;

    /// Write a file that stands for a binary.
    fn write_binary(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    /// Join directories into one `PATH` value.
    fn path_of(dirs: &[&Path]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn prefix_args_for_uv_runs_tool_run() {
        let uv = Uv {
            path: PathBuf::from("/usr/bin/uv"),
            kind: UvKind::Uv,
        };
        assert_eq!(uv.prefix_args(), &["tool", "run"]);
    }

    #[test]
    fn prefix_args_for_uvx_are_empty() {
        let uvx = Uv {
            path: PathBuf::from("/usr/bin/uvx"),
            kind: UvKind::Uvx,
        };
        assert!(uvx.prefix_args().is_empty());
    }

    #[test]
    fn path_search_finds_uv() {
        let empty = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let expected = write_binary(dir.path(), UV_NAME);

        let path = path_of(&[empty.path(), dir.path()]);
        let found = lookup(None, None, Some(&path)).unwrap();

        assert_eq!(found.path, expected);
        assert_eq!(found.kind, UvKind::Uv);
    }

    #[test]
    fn path_search_reads_the_first_entry_that_holds_uv() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let expected = write_binary(first.path(), UV_NAME);
        write_binary(second.path(), UV_NAME);

        let path = path_of(&[first.path(), second.path()]);
        let found = lookup(None, None, Some(&path)).unwrap();

        assert_eq!(found.path, expected);
    }

    #[test]
    fn path_search_reads_uvx_only_when_no_uv_exists() {
        let dir = tempfile::tempdir().unwrap();
        let expected = write_binary(dir.path(), UVX_NAME);

        let path = path_of(&[dir.path()]);
        let found = lookup(None, None, Some(&path)).unwrap();

        assert_eq!(found.path, expected);
        assert_eq!(found.kind, UvKind::Uvx);
    }

    #[test]
    fn uv_wins_over_uvx_in_the_same_directory() {
        let dir = tempfile::tempdir().unwrap();
        let expected = write_binary(dir.path(), UV_NAME);
        write_binary(dir.path(), UVX_NAME);

        let path = path_of(&[dir.path()]);
        let found = lookup(None, None, Some(&path)).unwrap();

        assert_eq!(found.path, expected);
        assert_eq!(found.kind, UvKind::Uv);
    }

    #[test]
    fn a_directory_with_the_name_of_the_binary_is_not_a_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(UV_NAME)).unwrap();

        let path = path_of(&[dir.path()]);
        let error = lookup(None, None, Some(&path)).unwrap_err();

        assert!(error.to_string().contains("Install uv"));
    }

    #[test]
    fn uvxy_uv_wins_over_uv_and_over_path() {
        let dir = tempfile::tempdir().unwrap();
        let explicit = write_binary(dir.path(), "explicit-uv");
        let from_env = write_binary(dir.path(), "env-uv");
        write_binary(dir.path(), UV_NAME);

        let path = path_of(&[dir.path()]);
        let found = lookup(
            Some(explicit.as_os_str()),
            Some(from_env.as_os_str()),
            Some(&path),
        )
        .unwrap();

        assert_eq!(found.path, explicit);
        assert_eq!(found.kind, UvKind::Uv);
    }

    #[test]
    fn uv_wins_over_path() {
        let dir = tempfile::tempdir().unwrap();
        let from_env = write_binary(dir.path(), "env-uv");
        write_binary(dir.path(), UV_NAME);

        let path = path_of(&[dir.path()]);
        let found = lookup(None, Some(from_env.as_os_str()), Some(&path)).unwrap();

        assert_eq!(found.path, from_env);
        assert_eq!(found.kind, UvKind::Uv);
    }

    #[test]
    fn an_empty_variable_counts_as_an_absent_variable() {
        let dir = tempfile::tempdir().unwrap();
        let expected = write_binary(dir.path(), UV_NAME);

        let path = path_of(&[dir.path()]);
        let empty = OsString::new();
        let found = lookup(Some(&empty), Some(&empty), Some(&path)).unwrap();

        assert_eq!(found.path, expected);
    }

    #[test]
    fn a_missing_uvxy_uv_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write_binary(dir.path(), UV_NAME);
        let absent = dir.path().join("absent-uv");

        let path = path_of(&[dir.path()]);
        let error = lookup(Some(absent.as_os_str()), None, Some(&path)).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("$UVXY_UV"), "{message}");
        assert!(message.contains("absent-uv"), "{message}");
    }

    #[test]
    fn a_missing_uv_variable_yields_the_path_search() {
        let dir = tempfile::tempdir().unwrap();
        let expected = write_binary(dir.path(), UV_NAME);
        let absent = dir.path().join("absent-uv");

        let path = path_of(&[dir.path()]);
        let found = lookup(None, Some(absent.as_os_str()), Some(&path)).unwrap();

        assert_eq!(found.path, expected);
    }

    #[test]
    fn no_binary_yields_an_error_that_names_the_installation() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_of(&[dir.path()]);

        let error = lookup(None, None, Some(&path)).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("Install uv"), "{message}");
    }

    #[test]
    fn an_absent_path_yields_an_error() {
        let error = lookup(None, None, None).unwrap_err();

        assert!(error.to_string().contains("Install uv"));
    }
}
