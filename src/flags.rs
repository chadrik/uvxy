//! Learn which uv flags take a value.
//!
//! See `docs/adr/0002-derive-flag-arity-from-shell-completions.md`.

use crate::uvbin::{Uv, UvKind};
use anyhow::Context;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The first line of a cache file. A change to the file format needs a change
/// to this string, because an old file then no longer matches the key.
const CACHE_FORMAT: &str = "uvxy-flags-1";

/// The `uv tool run` flags that consume the argument after them.
///
/// Names carry their dashes, as in `--from` and `-w`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagTable {
    value_flags: BTreeSet<String>,
}

// `rewrite` calls `takes_value`. The tests call the other three methods, and
// clippy requires `is_empty` next to `len`.
#[allow(dead_code)]
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

    /// Report whether the table holds no flag.
    pub fn is_empty(&self) -> bool {
        self.value_flags.is_empty()
    }
}

/// Return the flag table for `uv`, from the cache or from uv itself.
pub fn load(uv: &Uv) -> anyhow::Result<FlagTable> {
    // `uvx` reads its first argument as the name of a tool. A request for
    // completions therefore starts a tool named `generate-shell-completion`.
    if uv.kind == UvKind::Uvx {
        anyhow::bail!(
            "`{}` is uvx, and uvx cannot generate shell completions",
            uv.path.display()
        );
    }

    // One `stat` call yields the whole cache key. This is the steady-state cost
    // of the module.
    let metadata =
        fs::metadata(&uv.path).with_context(|| format!("cannot read `{}`", uv.path.display()))?;
    let key = cache_key(&uv.path, &metadata);
    let file = cache_file(&uv.path);

    if let Some(file) = &file
        && let Some(table) = read_cache(file, &key)
    {
        return Ok(table);
    }

    let table = derive(uv)?;

    // A cache directory that refuses writes costs speed, and nothing else.
    if let Some(file) = &file {
        let _ = write_cache(file, &key, &table);
    }

    Ok(table)
}

/// Run uv, and read the flag table out of the output.
fn derive(uv: &Uv) -> anyhow::Result<FlagTable> {
    let output = std::process::Command::new(&uv.path)
        .arg("generate-shell-completion")
        .arg("zsh")
        .output()
        .with_context(|| format!("cannot run `{}`", uv.path.display()))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`{} generate-shell-completion zsh` failed: {}",
            uv.path.display(),
            message.trim()
        );
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_zsh_completion(&text)
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
    let section = tool_run_section(text).context(
        "cannot find the `uv tool run` section of the zsh completion script. \
         The format of the script changed.",
    )?;

    let mut value_flags = BTreeSet::new();
    for line in section {
        if let Some((name, takes_value)) = parse_entry(line)
            && takes_value
        {
            value_flags.insert(name);
        }
    }

    if value_flags.is_empty() {
        anyhow::bail!(
            "the `uv tool run` section of the zsh completion script declares no \
             flag that takes a value. The format of the script changed."
        );
    }

    Ok(FlagTable { value_flags })
}

/// Return the lines that describe the arguments of `uv tool run`.
///
/// The script holds one `_arguments` call for every subcommand. The call for
/// `uv tool run` sits inside a `case` statement that a `curcontext` assignment
/// introduces. That assignment names `uv-tool-command-`, and it appears once.
/// The branch of the case statement carries the label `(run)`, and a `;;` line
/// closes it.
///
/// This scope matters. The label `(run)` also names `uv run`, and the flags of
/// `uv run` differ from the flags of `uv tool run`.
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

    // A section without a close reaches the next subcommand, so the flags of
    // that subcommand pollute the table. Report a failure instead.
    None
}

/// Read one `_arguments` entry.
///
/// Return the name of the flag, with its dashes, and whether the flag consumes
/// the argument after it.
///
/// An entry takes one of these forms:
///
/// ```text
/// '--from=[Use the given package to provide the command]:FROM:_default' \
/// '*-w+[Run with the given packages installed]:WITH:_default' \
/// '(--no-color)--color=[Control the use of color in output]:COLOR_CHOICE:((...
/// '--isolated[Run the tool in an isolated virtual environment]' \
/// ```
///
/// zsh writes `=` after a long flag that takes a value, and `+` after a short
/// flag that takes a value. A `:NAME:` marker follows the description of such
/// a flag. This function requires both signals.
fn parse_entry(line: &str) -> Option<(String, bool)> {
    // Every flag entry carries single quotes. A positional entry carries double
    // quotes, as in `"*::external_command:_default"`.
    let mut rest = line.trim().strip_prefix('\'')?;

    // A parenthesised list names the flags that this flag excludes.
    if let Some(after) = rest.strip_prefix('(') {
        let close = after.find(')')?;
        rest = &after[close + 1..];
    }

    // A `*` marks a repeatable flag. The arity does not change.
    rest = rest.strip_prefix('*').unwrap_or(rest);

    let separator = rest.find(['=', '+', '['])?;
    let name = &rest[..separator];
    if !is_flag_name(name) {
        return None;
    }

    let separator_byte = rest.as_bytes()[separator];
    let open = if separator_byte == b'[' {
        separator
    } else {
        separator + 1
    };
    if rest.as_bytes().get(open) != Some(&b'[') {
        return None;
    }

    let close = open + closing_bracket(&rest[open..])?;
    let after_description = &rest[close + 1..];
    let takes_value = separator_byte != b'[' && after_description.starts_with(':');

    Some((name.to_string(), takes_value))
}

/// Report whether this text names a flag.
fn is_flag_name(name: &str) -> bool {
    let body = match name.strip_prefix("--") {
        Some(body) => body,
        None => name.strip_prefix('-').unwrap_or(""),
    };
    !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Return the offset of the bracket that closes the first bracket.
///
/// `text` must start with `[`. A backslash escapes the character after it, so
/// a description that contains `\[` or `\]` does not disturb the count.
fn closing_bracket(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                index += 2;
                continue;
            }
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Build the key that identifies one uv binary.
///
/// The key holds the path, the modification time, and the size. A single
/// `stat` call yields all three values, and no uv process starts.
fn cache_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    format!(
        "{CACHE_FORMAT} {mtime} {} {}",
        metadata.len(),
        path.display()
    )
}

/// Return the directory that holds the cache files.
///
/// Return `None` when the environment names no home directory.
fn cache_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        if local.is_empty() {
            return None;
        }
        Some(PathBuf::from(local).join("uvxy").join("cache"))
    }

    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME")
            && !xdg.is_empty()
        {
            return Some(PathBuf::from(xdg).join("uvxy"));
        }
        let home = std::env::var_os("HOME")?;
        if home.is_empty() {
            return None;
        }
        Some(PathBuf::from(home).join(".cache").join("uvxy"))
    }
}

/// Return the path of the cache file for one uv binary.
///
/// The name carries a hash of the path, so two uv binaries hold two files and
/// neither one displaces the other.
fn cache_file(uv_path: &Path) -> Option<PathBuf> {
    let hash = hash_path(uv_path);
    Some(cache_dir()?.join(format!("flags-{hash:016x}")))
}

/// Hash a path with the FNV-1a function.
///
/// The hash names a cache file. It guards no data, so a short non-cryptographic
/// function is enough.
fn hash_path(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Read a cached table.
///
/// Return `None` when the file is absent, or when the key of the file differs
/// from the key of the uv binary. Every failure here is silent, because the
/// caller derives the table again.
fn read_cache(file: &Path, key: &str) -> Option<FlagTable> {
    let text = fs::read_to_string(file).ok()?;
    let mut lines = text.lines();
    if lines.next()? != key {
        return None;
    }
    let value_flags: BTreeSet<String> = lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if value_flags.is_empty() {
        return None;
    }
    Some(FlagTable { value_flags })
}

/// Write a table to the cache.
///
/// Write a temporary file in the target directory, then rename it. A rename
/// within one directory is atomic, so a reader sees either the old file or the
/// new file, and never a partial file.
fn write_cache(file: &Path, key: &str, table: &FlagTable) -> std::io::Result<()> {
    let dir = file.parent().ok_or(std::io::ErrorKind::NotFound)?;
    fs::create_dir_all(dir)?;

    let mut text = String::with_capacity(key.len() + table.len() * 24);
    text.push_str(key);
    text.push('\n');
    for flag in &table.value_flags {
        text.push_str(flag);
        text.push('\n');
    }

    // The name of the temporary file must differ for every process, because two
    // `uvxy` processes can write at the same moment.
    let stem = file.file_name().unwrap_or_default().to_string_lossy();
    let temp = dir.join(format!("{stem}.{}.tmp", std::process::id()));
    fs::write(&temp, text.as_bytes())?;
    match fs::rename(&temp, file) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An excerpt of the real output of `uv generate-shell-completion zsh`,
    /// from uv 0.8.17.
    ///
    /// The excerpt holds three sections. The first section belongs to `uv run`,
    /// the second section belongs to `uv tool run`, and the third section
    /// belongs to `uvx`. Only the second section may reach the table.
    const ZSH_COMPLETION: &str = r##"#compdef uv

_uv() {
    _arguments "${_arguments_options[@]}" : \
'--cache-dir=[Path to the cache directory]:CACHE_DIR:_files' \
":: :_uv_commands" \
"*::: :->uv" \
&& ret=0
    case $state in
    (uv)
        curcontext="${curcontext%:*:*}:uv-command-$line[1]:"
        case $line[1] in
            (run)
_arguments "${_arguments_options[@]}" : \
'*--only-group=[Only include dependencies from the specified dependency group]:ONLY_GROUP:_default' \
'--decoy-uv-run=[This flag belongs to uv run, and not to uv tool run]:DECOY:_default' \
'--no-sync[Avoid syncing the virtual environment]' \
"*::command:_default" \
&& ret=0
;;
(tool)
_arguments "${_arguments_options[@]}" : \
":: :_uv__tool_commands" \
"*::: :->tool" \
&& ret=0

    case $state in
    (tool)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:uv-tool-command-$line[1]:"
        case $line[1] in
            (run)
_arguments "${_arguments_options[@]}" : \
'--from=[Use the given package to provide the command]:FROM:_default' \
'*-w+[Run with the given packages installed]:WITH:_default' \
'*--with=[Run with the given packages installed]:WITH:_default' \
'*-c+[Constrain versions using the given requirements files]:CONSTRAINTS:_default' \
'*--constraints=[Constrain versions using the given requirements files]:CONSTRAINTS:_default' \
'*--env-file=[Load environment variables from a \`.env\` file]:ENV_FILE:_files' \
'-i+[(Deprecated\: use \`--default-index\` instead) The URL of the Python package index (by default\: <https\://pypi.org/simple>)]:INDEX_URL:_default' \
'--index-url=[(Deprecated\: use \`--default-index\` instead) The URL of the Python package index (by default\: <https\://pypi.org/simple>)]:INDEX_URL:_default' \
'--link-mode=[The method to use when installing packages from the global cache]:LINK_MODE:((clone\:"Clone (i.e., copy-on-write) packages from the wheel into the \`site-packages\` directory"
copy\:"Copy packages from the wheel into the \`site-packages\` directory"
hardlink\:"Hard link packages from the wheel into the \`site-packages\` directory"
symlink\:"Symbolically link packages from the wheel into the \`site-packages\` directory"))' \
'-p+[The Python interpreter to use to build the run environment.]:PYTHON:_default' \
'--python=[The Python interpreter to use to build the run environment.]:PYTHON:_default' \
'--generate-shell-completion=[]:GENERATE_SHELL_COMPLETION:(bash elvish fish nushell powershell zsh)' \
'(--no-color)--color=[Control the use of color in output]:COLOR_CHOICE:((auto\:"Enables colored output only when the output is going to a terminal or TTY with support"
always\:"Enables colored output regardless of the detected environment"
never\:"Disables colored output"))' \
'--isolated[Run the tool in an isolated virtual environment, ignoring any already-installed tools]' \
'--no-env-file[Avoid reading environment variables from a \`.env\` file]' \
'-U[Allow package upgrades, ignoring pinned versions in any existing output file. Implies \`--refresh\`]' \
'--upgrade[Allow package upgrades, ignoring pinned versions in any existing output file. Implies \`--refresh\`]' \
'(--offline)--refresh[Refresh all cached data]' \
'-n[Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation]' \
'(-v --verbose)*-q[Use quiet output]' \
'(-q --quiet)*--verbose[Use verbose output]' \
'--no-python-downloads[Disable automatic downloads of Python. \[env\: "UV_PYTHON_DOWNLOADS=never"\]]' \
'--offline[Disable network access]' \
'--no-config[Avoid discovering configuration files (\`pyproject.toml\`, \`uv.toml\`)]' \
'-h[Display the concise help for this command]' \
'--help[Display the concise help for this command]' \
"*::external_command:_default" \
&& ret=0
;;
(uvx)
_arguments "${_arguments_options[@]}" : \
'--decoy-uvx=[This flag belongs to the uvx section, and not to uv tool run]:DECOY:_default' \
&& ret=0
;;
        esac
    ;;
esac
}
"##;

    fn table() -> FlagTable {
        parse_zsh_completion(ZSH_COMPLETION).expect("the excerpt yields a table")
    }

    #[test]
    fn long_flags_take_a_value() {
        let table = table();
        for flag in [
            "--from",
            "--with",
            "--constraints",
            "--env-file",
            "--index-url",
            "--link-mode",
            "--python",
            "--color",
        ] {
            assert!(table.takes_value(flag), "{flag} takes a value");
        }
    }

    #[test]
    fn short_flags_take_a_value() {
        let table = table();
        for flag in ["-w", "-c", "-i", "-p"] {
            assert!(table.takes_value(flag), "{flag} takes a value");
        }
    }

    #[test]
    fn boolean_flags_take_no_value() {
        let table = table();
        for flag in [
            "--isolated",
            "--no-config",
            "--offline",
            "--no-env-file",
            "--upgrade",
            "--refresh",
            "--verbose",
            "--no-python-downloads",
            "--help",
            "-U",
            "-n",
            "-q",
            "-h",
        ] {
            assert!(!table.takes_value(flag), "{flag} takes no value");
        }
    }

    #[test]
    fn a_hidden_flag_reaches_the_table() {
        // `uv tool run --help` hides this flag. The completions declare it.
        assert!(table().takes_value("--generate-shell-completion"));
    }

    #[test]
    fn the_table_holds_only_the_tool_run_section() {
        let table = table();
        assert!(!table.takes_value("--decoy-uv-run"));
        assert!(!table.takes_value("--decoy-uvx"));
        assert!(!table.takes_value("--only-group"));
        assert!(!table.takes_value("--cache-dir"));
    }

    #[test]
    fn a_repeatable_flag_reaches_the_table_without_its_star() {
        let table = table();
        assert!(table.takes_value("--with"));
        assert!(!table.takes_value("*--with"));
    }

    #[test]
    fn text_without_the_section_yields_an_error() {
        assert!(parse_zsh_completion("").is_err());
        assert!(parse_zsh_completion("#compdef uv\n_uv() {\n}\n").is_err());
    }

    #[test]
    fn a_section_without_a_close_yields_an_error() {
        let text = "curcontext=\"uv-tool-command-$line[1]:\"\n\
                    (run)\n\
                    '--from=[Use the given package]:FROM:_default' \\\n";
        assert!(parse_zsh_completion(text).is_err());
    }

    #[test]
    fn a_section_with_no_value_flag_yields_an_error() {
        let text = "curcontext=\"uv-tool-command-$line[1]:\"\n\
                    (run)\n\
                    '--isolated[Run the tool in an isolated environment]' \\\n\
                    ;;\n";
        assert!(parse_zsh_completion(text).is_err());
    }

    #[test]
    fn a_description_with_escaped_brackets_parses() {
        let line = r#"'--no-python-downloads[Disable automatic downloads. \[env\: "X=never"\]]' \"#;
        assert_eq!(
            parse_entry(line),
            Some(("--no-python-downloads".to_string(), false))
        );
    }

    #[test]
    fn a_positional_entry_parses_to_nothing() {
        assert_eq!(parse_entry("\"*::external_command:_default\" \\"), None);
        assert_eq!(parse_entry("&& ret=0"), None);
        assert_eq!(parse_entry(";;"), None);
    }

    #[test]
    fn the_cache_holds_the_table_under_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("flags-0000000000000000");
        let table = FlagTable::from_flags(["--from", "--with", "-w"]);

        write_cache(&file, "key-a", &table).unwrap();

        assert_eq!(read_cache(&file, "key-a"), Some(table));
        assert_eq!(read_cache(&file, "key-b"), None);
    }

    #[test]
    fn the_cache_creates_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested").join("flags-1");
        let table = FlagTable::from_flags(["--from"]);

        write_cache(&file, "key", &table).unwrap();

        assert!(file.is_file());
        assert_eq!(read_cache(&file, "key"), Some(table));
    }

    #[test]
    fn the_cache_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("flags-2");
        write_cache(&file, "key", &FlagTable::from_flags(["--from"])).unwrap();

        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["flags-2".to_string()]);
    }

    #[test]
    fn a_missing_cache_file_reads_as_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_cache(&dir.path().join("absent"), "key"), None);
    }

    #[test]
    fn two_uv_paths_name_two_cache_files() {
        let first = cache_file(Path::new("/usr/bin/uv"));
        let second = cache_file(Path::new("/opt/uv/bin/uv"));
        if let (Some(first), Some(second)) = (first, second) {
            assert_ne!(first, second);
        }
    }

    #[test]
    fn uvx_yields_an_error() {
        let uvx = Uv {
            path: PathBuf::from("/usr/bin/uvx"),
            kind: UvKind::Uvx,
        };
        assert!(load(&uvx).is_err());
    }
}
