//! End to end tests. Each test starts the compiled `uvxy` binary.
//!
//! The unit tests inside `src/` cover the modules. These tests cover the layer
//! that a unit test cannot reach. They check the arguments that reach `uv`,
//! the exit code, and the two output streams.
//!
//! Every test points `UVXY_UV` at a fake `uv`, so no real tool ever runs. The
//! fake is a shell script, so these tests run on Unix only.
#![cfg(unix)]

mod common;

use common::{Fixture, SEED_FLAGS, as_strs, stderr, stdout};

/// The text that `uvxy` writes when it cannot read the uv flags.
const WARNING: &str = "cannot read the uv flags";

// ---------------------------------------------------------------------------
// The fallback rule
//
// The fake `uv` writes no completion script, so `flags::load` fails. `uvxy`
// then warns and reads the first argument as the command. See ADR 0002. Each
// test in this group is correct under that rule.
// ---------------------------------------------------------------------------

#[test]
fn a_mapped_command_gets_a_from_argument() {
    let fixture = Fixture::new();

    let output = fixture.run(&["sphinx-build", "docs", "out"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--from", "sphinx", "sphinx-build", "docs", "out"]
    );
}

#[test]
fn a_failed_flag_table_warns_and_still_runs_uv() {
    let fixture = Fixture::new();

    let output = fixture.run(&["sphinx-build"]);

    let message = stderr(&output);
    assert!(message.contains(WARNING), "{message}");
    assert!(
        message.contains("first argument as the command"),
        "{message}"
    );
    assert!(output.status.success(), "{message}");
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--from", "sphinx", "sphinx-build"]
    );
}

#[test]
fn an_unmapped_command_passes_through() {
    let fixture = Fixture::new();

    let output = fixture.run(&["ruff", "check", "--fix"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let args = fixture.tool_run();
    assert_eq!(as_strs(&args), ["ruff", "check", "--fix"]);
    assert!(!args.iter().any(|arg| arg == "--from"), "{args:?}");
}

#[test]
fn the_from_argument_goes_before_the_separator() {
    let fixture = Fixture::new();

    let output = fixture.run(&["--", "sphinx-build", "--help"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--from", "sphinx", "--", "sphinx-build", "--help"]
    );
}

#[test]
fn a_version_on_the_command_moves_to_the_package_spec() {
    let fixture = Fixture::new();

    let output = fixture.run(&["mytool@1.2", "--flag"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--from", "acme-mytool@1.2", "mytool", "--flag"]
    );
}

#[test]
fn two_version_pins_yield_an_error_that_names_both() {
    let fixture = Fixture::new();

    let output = fixture.run(&["pinned@3.0"]);

    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("acme-pinned==2.1"), "{message}");
    assert!(message.contains("@3.0"), "{message}");
    assert!(!fixture.uv_ran_a_tool(), "uv must not run a tool");
}

#[test]
fn a_missing_configuration_file_gives_a_passthrough() {
    // A system file would supply a mapping, and the assertion below would then
    // read the wrong reason for a failure.
    if std::path::Path::new("/etc/uv/uvxy.toml").exists() {
        eprintln!("skip: this machine holds /etc/uv/uvxy.toml");
        return;
    }
    let fixture = Fixture::new();

    let output = fixture.run_without_config(&["sphinx-build", "docs"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(as_strs(&fixture.tool_run()), ["sphinx-build", "docs"]);
}

// ---------------------------------------------------------------------------
// The real flag table
//
// Each test in this group seeds the flag cache, so `flags::load` reads the
// cache and starts no process. The fake `uv` then records the handoff only,
// and `uvxy` knows the arity of every flag.
// ---------------------------------------------------------------------------

#[test]
fn a_value_flag_before_the_command_does_not_read_as_the_command() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--with", "sphinx-rtd-theme", "sphinx-build", "docs"]);

    let message = stderr(&output);
    assert!(output.status.success(), "{message}");
    // A seeded cache costs no uv process, so no warning reaches the user.
    assert!(!message.contains(WARNING), "{message}");
    assert_eq!(
        as_strs(&fixture.only_run()),
        [
            "tool",
            "run",
            "--with",
            "sphinx-rtd-theme",
            "--from",
            "sphinx",
            "sphinx-build",
            "docs",
        ]
    );
}

#[test]
fn a_command_that_repeats_a_flag_value_still_resolves() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--with", "black", "black"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--with", "black", "--from", "black-nightly", "black"]
    );
}

#[test]
fn a_user_from_argument_suppresses_the_mapping() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--from", "other-package", "sphinx-build", "docs"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let args = fixture.tool_run();
    assert_eq!(
        as_strs(&args),
        ["--from", "other-package", "sphinx-build", "docs"]
    );
    assert_eq!(args.iter().filter(|arg| *arg == "--from").count(), 1);
}

#[test]
fn a_flag_that_holds_its_own_value_frees_the_next_argument() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--python=3.12", "mytool"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--python=3.12", "--from", "acme-mytool", "mytool"]
    );
}

// ---------------------------------------------------------------------------
// The namespaced flags
//
// Each of these flags answers without a handoff. The tests seed the cache, so
// `uvxy` starts no process at all. The record file therefore proves that the
// fake `uv` never ran.
// ---------------------------------------------------------------------------

#[test]
fn the_version_flag_prints_the_version_and_runs_no_uv() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-version"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output).trim(),
        format!("uvxy {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn the_help_flag_prints_the_usage_and_runs_no_uv() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-help"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Usage: uvxy"), "{text}");
    assert!(text.contains("--uvxy-explain"), "{text}");
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn the_explain_flag_prints_the_planned_command_and_runs_no_uv() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-explain", "--with", "furo", "sphinx-build", "docs"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(&format!(
            "{} tool run --with furo --from sphinx sphinx-build docs",
            fixture.uv.display()
        )),
        "{text}"
    );
    assert!(text.contains("command: sphinx-build"), "{text}");
    assert!(
        text.contains("mapping: sphinx-build = \"sphinx\""),
        "{text}"
    );
    assert!(
        text.contains(&format!("source:  {}", fixture.config.display())),
        "{text}"
    );
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn an_unknown_namespaced_flag_yields_an_error() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-bogus", "sphinx-build"]);

    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("--uvxy-bogus"), "{message}");
    assert!(message.contains("--uvxy-help"), "{message}");
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn a_namespaced_flag_after_the_command_reaches_the_command() {
    let fixture = Fixture::new();
    fixture.seed_flag_cache(SEED_FLAGS);

    // ADR 0003: uvxy reads a namespaced flag only before the command.
    let output = fixture.run(&["mytool", "--uvxy-bogus"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        as_strs(&fixture.tool_run()),
        ["--from", "acme-mytool", "mytool", "--uvxy-bogus"]
    );
}

// ---------------------------------------------------------------------------
// A broken configuration file
// ---------------------------------------------------------------------------

/// A file that TOML cannot parse.
const BROKEN_CONFIG: &str = "[from\nsphinx-build = \n";

#[test]
fn a_malformed_configuration_file_yields_an_error_that_names_the_file() {
    let fixture = Fixture::with_config(BROKEN_CONFIG);
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["sphinx-build"]);

    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains(&fixture.config.display().to_string()),
        "{message}"
    );
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn the_help_flag_answers_when_the_configuration_file_is_malformed() {
    // `run` in src/main.rs holds the configuration error. A user who asks for
    // help must get help, and not the error that the help explains.
    let fixture = Fixture::with_config(BROKEN_CONFIG);
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-help"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Usage: uvxy"), "{output:?}");
    assert!(!fixture.uv_ran(), "uv must not run");
}

#[test]
fn the_explain_flag_reports_a_malformed_configuration_file() {
    let fixture = Fixture::with_config(BROKEN_CONFIG);
    fixture.seed_flag_cache(SEED_FLAGS);

    let output = fixture.run(&["--uvxy-explain", "sphinx-build"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("cannot read the configuration file"),
        "{text}"
    );
    assert!(text.contains("mapping: none applied"), "{text}");
    assert!(!fixture.uv_ran(), "uv must not run");
}
