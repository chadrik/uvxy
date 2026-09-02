//! Helpers that drive the compiled `uvxy` binary against a fake `uv`.
//!
//! `uvxy` is a binary crate, so a test cannot call its modules. Every test
//! here starts the real binary and reads what the binary does.
//!
//! Each test file uses a part of this module, so some items look unused.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// The path of the compiled binary. Cargo sets this variable.
pub const UVXY: &str = env!("CARGO_BIN_EXE_uvxy");

/// The line that marks the start of one recorded invocation.
const RUN_MARKER: &str = "##run";

/// The first line of a flag cache file. `src/flags.rs` writes this string.
const CACHE_FORMAT: &str = "uvxy-flags-1";

/// The flags that a seeded cache declares as value flags.
///
/// uv 0.8.17 declares more flags than this list. A test needs only the flags
/// that it types.
pub const SEED_FLAGS: &[&str] = &[
    "--from",
    "--with",
    "-w",
    "--python",
    "-p",
    "--constraints",
    "-c",
    "--index-url",
    "-i",
    "--color",
    "--link-mode",
    "--env-file",
    "--generate-shell-completion",
];

/// The mappings that most tests read.
pub const DEFAULT_CONFIG: &str = "\
[commands]
sphinx-build = \"sphinx\"
mytool = \"acme-mytool\"
pinned = \"acme-pinned==2.1\"
black = \"black-nightly\"
";

/// One test environment.
///
/// The environment holds a fake `uv`, a record file, a configuration file, and
/// a cache directory. The temporary directory holds all four, and it goes away
/// when the fixture goes away.
pub struct Fixture {
    _dir: TempDir,
    /// The fake `uv` binary.
    pub uv: PathBuf,
    /// The file that receives the arguments of every fake `uv` invocation.
    pub record: PathBuf,
    /// The configuration file that `UVXY_CONFIG_FILE` names.
    pub config: PathBuf,
    /// The directory that `XDG_CACHE_HOME` names.
    pub cache: PathBuf,
    /// An empty directory that stands for a home directory.
    pub home: PathBuf,
}

impl Fixture {
    /// Build a fixture that holds the default mappings.
    pub fn new() -> Self {
        Self::with_config(DEFAULT_CONFIG)
    }

    /// Build a fixture, and write `config_text` to the configuration file.
    pub fn with_config(config_text: &str) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = dir.path().to_path_buf();

        let uv = root.join("fake-uv");
        let record = root.join("uv-argv.txt");
        let config = root.join("uvxy.toml");
        let cache = root.join("cache");
        let home = root.join("home");

        std::fs::create_dir(&home).expect("a home directory");
        std::fs::write(&config, config_text).expect("a configuration file");
        write_fake_uv(&uv, &record);

        Self {
            _dir: dir,
            uv,
            record,
            config,
            cache,
            home,
        }
    }

    /// Build a command that starts `uvxy` with a controlled environment.
    ///
    /// The test sets every variable on the command. A test never writes the
    /// environment of the test process, because the test threads share it.
    fn base_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(UVXY);
        command.args(args);
        command.env("UVXY_UV", &self.uv);
        command.env("XDG_CACHE_HOME", &self.cache);
        // The environment of the developer must not reach the binary.
        command.env_remove("UV");
        command.env_remove("UVXY_CONFIG_FILE");
        command.env_remove("XDG_CONFIG_HOME");
        command
    }

    /// Run `uvxy` against the configuration file of this fixture.
    pub fn run(&self, args: &[&str]) -> Output {
        self.base_command(args)
            .env("UVXY_CONFIG_FILE", &self.config)
            .output()
            .expect("uvxy starts")
    }

    /// Run `uvxy` with no configuration file at all.
    ///
    /// The home directory and the configuration directory are empty, so `uvxy`
    /// finds no user file.
    pub fn run_without_config(&self, args: &[&str]) -> Output {
        self.base_command(args)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("HOME", &self.home)
            .output()
            .expect("uvxy starts")
    }

    /// Write a flag cache that matches the fake `uv`.
    ///
    /// `src/flags.rs` reads this file, and it then starts no process. Two
    /// results follow. The real flag table applies, and the fake `uv` records
    /// only the handoff.
    pub fn seed_flag_cache(&self, flags: &[&str]) {
        let metadata = std::fs::metadata(&self.uv).expect("the fake uv exists");
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let key = format!(
            "{CACHE_FORMAT} {mtime} {} {}",
            metadata.len(),
            self.uv.display()
        );

        let mut text = key;
        text.push('\n');
        for flag in flags {
            text.push_str(flag);
            text.push('\n');
        }

        let dir = self.cache.join("uvxy");
        std::fs::create_dir_all(&dir).expect("a cache directory");
        let file = dir.join(format!("flags-{:016x}", fnv1a(&self.uv)));
        std::fs::write(file, text).expect("a cache file");
    }

    /// Report whether the fake `uv` ran at all.
    pub fn uv_ran(&self) -> bool {
        self.record.exists()
    }

    /// Report whether the fake `uv` received a handoff.
    ///
    /// A run without a flag cache also holds one completion invocation. That
    /// invocation is not a handoff.
    pub fn uv_ran_a_tool(&self) -> bool {
        self.runs()
            .iter()
            .any(|run| run.first().map(String::as_str) == Some("tool"))
    }

    /// Return the arguments of every fake `uv` invocation, in order.
    pub fn runs(&self) -> Vec<Vec<String>> {
        let Ok(text) = std::fs::read_to_string(&self.record) else {
            return Vec::new();
        };
        let mut runs: Vec<Vec<String>> = Vec::new();
        for line in text.lines() {
            if line == RUN_MARKER {
                runs.push(Vec::new());
            } else if let Some(current) = runs.last_mut() {
                current.push(line.to_string());
            }
        }
        runs
    }

    /// Return the arguments of the single fake `uv` invocation.
    ///
    /// Fail when the fake ran a number of times other than one.
    pub fn only_run(&self) -> Vec<String> {
        let runs = self.runs();
        assert_eq!(runs.len(), 1, "the fake uv ran once, and got {runs:?}");
        runs.into_iter().next().expect("one run")
    }

    /// Return the arguments of the invocation that carries `tool run`.
    ///
    /// A run without a flag cache also holds one completion invocation. This
    /// method reads past that invocation.
    pub fn tool_run(&self) -> Vec<String> {
        let runs = self.runs();
        let mut matches = runs
            .iter()
            .filter(|run| run.first().map(String::as_str) == Some("tool"));
        let found = matches
            .next()
            .unwrap_or_else(|| panic!("one `tool run` invocation, and got {runs:?}"));
        assert!(
            matches.next().is_none(),
            "one `tool run` invocation, and got {runs:?}"
        );
        assert_eq!(found[..2], ["tool".to_string(), "run".to_string()]);
        found[2..].to_vec()
    }
}

/// Write a fake `uv` that records its arguments and exits 0.
///
/// The script holds the path of the record file, so the fake needs no
/// environment variable of its own.
fn write_fake_uv(path: &Path, record: &Path) {
    let script = format!(
        "#!/bin/sh\n\
         # This fake uv records one block for every invocation.\n\
         {{\n\
         \tprintf '{RUN_MARKER}\\n'\n\
         \tfor arg in \"$@\"; do\n\
         \t\tprintf '%s\\n' \"$arg\"\n\
         \tdone\n\
         }} >> '{}'\n\
         exit 0\n",
        record.display()
    );
    std::fs::write(path, script).expect("the fake uv");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("an executable fake uv");
    }
}

/// Hash a path with the FNV-1a function.
///
/// `src/flags.rs` names each cache file after this hash. A test that seeds the
/// cache must build the same name.
fn fnv1a(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Return the standard output of a process as text.
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Return the standard error of a process as text.
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Turn a list of strings into a list of borrowed strings, for a comparison.
pub fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}
