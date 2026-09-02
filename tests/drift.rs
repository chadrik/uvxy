//! Watch the uv shell completion format.
//!
//! `uvxy` reads the arity of each `uv tool run` flag from
//! `uv generate-shell-completion zsh`. See
//! `docs/adr/0002-derive-flag-arity-from-shell-completions.md`. That output
//! belongs to uv, and uv can change it in any release.
//!
//! This test runs the real uv. It therefore skips itself unless `UVXY_DRIFT`
//! holds `1`. The drift workflow sets that variable, and it installs the
//! newest uv release first.
//!
//! A failure here means that uv changed its completion format. It does not
//! mean that `uvxy` changed.
#![cfg(unix)]

mod common;

use common::{UVXY, stderr, stdout};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The sentence that every failure in this file repeats.
const DRIFT: &str = "uv changed the format of its shell completion script. \
                     Read `uv generate-shell-completion zsh`, then update the \
                     parser in `src/flags.rs`.";

/// Report whether the caller asked for the drift check.
fn asked_for_drift() -> bool {
    std::env::var("UVXY_DRIFT").as_deref() == Ok("1")
}

/// Find the real `uv` on `PATH`.
fn find_uv() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("uv"))
        .find(|candidate| candidate.is_file())
}

/// Run the real uv, and return the zsh completion script.
fn completion_script(uv: &Path) -> String {
    let output = Command::new(uv)
        .args(["generate-shell-completion", "zsh"])
        .output()
        .unwrap_or_else(|error| panic!("cannot run `{}`: {error}", uv.display()));
    assert!(
        output.status.success(),
        "`{} generate-shell-completion zsh` failed. {DRIFT}\n{}",
        uv.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("the script is UTF-8")
}

/// Return the lines of the `uv tool run` section.
///
/// `tool_run_section` in `src/flags.rs` anchors on the same three markers. The
/// `uv-tool-command-` assignment opens the scope, the `(run)` label opens the
/// branch, and a `;;` line closes it.
fn tool_run_section(text: &str) -> Option<Vec<&str>> {
    let marker = text.find("uv-tool-command-")?;
    let mut lines = text[marker..].lines();
    lines.find(|line| line.trim() == "(run)")?;

    let mut section = Vec::new();
    for line in lines {
        if line.trim() == ";;" {
            return Some(section);
        }
        section.push(line);
    }
    None
}

/// The primary check. It proves that the whole chain still works.
///
/// `uvxy --with somepkg mytool` holds one command, `mytool`. `uvxy` finds that
/// command only when it knows that `--with` consumes a value. A format change
/// breaks `flags::load`, `uvxy` then reads the first argument as the command,
/// `--with` starts with a dash, and no `--from` reaches the output.
///
/// This test therefore covers the whole chain: run uv, parse the script, write
/// the cache, scan the arguments, and insert `--from`.
#[test]
fn drift_the_real_uv_still_yields_a_correct_flag_table() {
    let Some(uv) = require_uv() else { return };

    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("uvxy.toml");
    std::fs::write(&config, "[commands]\nmytool = \"acme-mytool\"\n")
        .expect("a configuration file");

    // `--uvxy-explain` prints the plan and runs no tool, so this test installs
    // nothing and reaches no network.
    let output = Command::new(UVXY)
        .args(["--uvxy-explain", "--with", "somepkg", "mytool"])
        .env("UVXY_UV", &uv)
        .env("UVXY_CONFIG_FILE", &config)
        // An empty cache directory forces uvxy to read the real uv.
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env_remove("UV")
        .output()
        .expect("uvxy starts");

    let text = stdout(&output);
    let message = stderr(&output);
    assert!(output.status.success(), "{DRIFT}\n{message}");
    assert!(
        !message.contains("cannot read the uv flags"),
        "uvxy could not build a flag table from `{}`. {DRIFT}\n{message}",
        uv.display()
    );
    assert!(
        text.contains("--from acme-mytool mytool"),
        "uvxy did not insert `--from` before `mytool`, so it does not know \
         that `--with` consumes a value. {DRIFT}\n{text}"
    );
    assert!(
        text.contains("command: mytool"),
        "uvxy read the wrong command. {DRIFT}\n{text}"
    );
}

/// The command repeats the value of `--with`. Only correct arity separates the
/// two words, and the second word is the command.
#[test]
fn drift_a_flag_value_that_repeats_the_command_still_resolves() {
    let Some(uv) = require_uv() else { return };

    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("uvxy.toml");
    std::fs::write(&config, "[commands]\nmytool = \"acme-mytool\"\n")
        .expect("a configuration file");

    let output = Command::new(UVXY)
        .args(["--uvxy-explain", "--with", "mytool", "mytool"])
        .env("UVXY_UV", &uv)
        .env("UVXY_CONFIG_FILE", &config)
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env_remove("UV")
        .output()
        .expect("uvxy starts");

    let text = stdout(&output);
    assert!(output.status.success(), "{DRIFT}\n{}", stderr(&output));
    assert!(
        text.contains("tool run --with mytool --from acme-mytool mytool"),
        "uvxy read the first `mytool` as the command, so it does not know that \
         `--with` consumes a value. {DRIFT}\n{text}"
    );
}

/// The secondary check. It names the marker that moved.
///
/// The primary check reports that the chain broke. This check reports which
/// part of the format the break belongs to.
#[test]
fn drift_the_completion_script_still_holds_the_arity_markers() {
    let Some(uv) = require_uv() else { return };
    let script = completion_script(&uv);

    assert!(
        script.contains("uv-tool-command-"),
        "the script holds no `uv-tool-command-` assignment, so `uvxy` cannot \
         find the `uv tool run` section. {DRIFT}"
    );

    let section = tool_run_section(&script).unwrap_or_else(|| {
        panic!(
            "the script holds no `uv tool run` section that a `(run)` label \
             opens and a `;;` line closes. {DRIFT}"
        )
    });

    // `'--from=[Use the given package to provide the command]:FROM:_default'`
    // The `=` and the `:FROM:` marker together declare that `--from` consumes
    // the argument after it.
    let from = section
        .iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("'--from=["));
    let from = from.unwrap_or_else(|| {
        panic!("the `uv tool run` section declares no `'--from=[...]` entry. {DRIFT}")
    });
    assert!(
        from.contains("]:FROM:_default'"),
        "the `--from` entry no longer carries the `:FROM:_default` arity \
         marker. {DRIFT}\n{from}"
    );

    // `'*--with=[Run with the given packages installed]:WITH:_default'`
    // The leading `*` marks a repeatable flag. The arity marker follows the
    // description, exactly as it does for a flag that appears once.
    let with = section
        .iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("'*--with=["));
    let with = with.unwrap_or_else(|| {
        panic!(
            "the `uv tool run` section declares no repeatable \
             `'*--with=[...]` entry. {DRIFT}"
        )
    });
    assert!(
        with.contains("]:WITH:_default'"),
        "the `--with` entry no longer carries the `:WITH:_default` arity \
         marker. {DRIFT}\n{with}"
    );

    // A flag that takes no value carries no marker. The parser reads the
    // difference, so the absence matters as much as the presence.
    let isolated = section
        .iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("'--isolated["));
    if let Some(isolated) = isolated {
        assert!(
            isolated.ends_with("]' \\") || isolated.ends_with("]'"),
            "the `--isolated` entry now carries text after its description, \
             and `uvxy` reads that text as an arity marker. {DRIFT}\n{isolated}"
        );
    }
}

/// Return the path of the real uv, or skip this test.
///
/// The test skips when the caller did not ask for the drift check, and when
/// the machine holds no uv. A skip is not a failure. `cargo test` has no skip,
/// so the test prints a line and returns.
fn require_uv() -> Option<PathBuf> {
    if !asked_for_drift() {
        eprintln!("skip: the drift check runs only under UVXY_DRIFT=1");
        return None;
    }
    match find_uv() {
        Some(uv) => Some(uv),
        None => {
            eprintln!("skip: this machine holds no `uv` on PATH");
            None
        }
    }
}
