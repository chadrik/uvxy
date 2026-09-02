//! Find the mappings, and read them.
//!
//! See `docs/adr/0001-separate-config-file-no-directory-search.md`.

use crate::Mappings;
use anyhow::{Context, anyhow};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file that holds the mappings.
const FILE_NAME: &str = "uvxy.toml";

/// The directory that holds the system file.
#[cfg(not(windows))]
const SYSTEM_DIR: &str = "/etc/uv";

/// The variable that replaces every path.
const CONFIG_FILE_VAR: &str = "UVXY_CONFIG_FILE";

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
    load_from(
        env_value(CONFIG_FILE_VAR).map(PathBuf::from),
        &config_paths(),
    )
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
    #[cfg(windows)]
    {
        windows_paths(env_value("APPDATA").as_deref())
    }
    #[cfg(not(windows))]
    {
        unix_paths(
            env_value("XDG_CONFIG_HOME").as_deref(),
            env_value("HOME").as_deref(),
        )
    }
}

/// Read the `[commands]` table out of one file's text.
///
/// Each key is a command. Each value is a package spec string. Match a key
/// exactly, and never normalize it. Return an error when the text does not
/// parse, when the file holds an unknown table, when `[commands]` is not a
/// table, or when a value is not a string.
pub fn parse(text: &str) -> anyhow::Result<Mappings> {
    let document: toml::Table = text.parse().context("the text is not valid TOML")?;

    // uvxy reads a closed list of names, as uv does for `uv.toml`. uvxy
    // rejects an unknown name rather than skip it. A skipped mapping runs the
    // wrong package, and it reports nothing.
    for key in document.keys() {
        if key != "commands" {
            return Err(anyhow!(
                "`{key}` is not a name that uvxy reads. uvxy reads one table, and that table is `commands`."
            ));
        }
    }

    // A file without a `[commands]` table holds no mapping. That is not an
    // error.
    let Some(commands) = document.get("commands") else {
        return Ok(Mappings::new());
    };
    let commands = commands.as_table().ok_or_else(|| {
        anyhow!(
            "`commands` holds {}, and `commands` must hold a table",
            article(commands.type_str())
        )
    })?;

    let mut mappings = Mappings::new();
    for (command, spec) in commands {
        let spec = spec.as_str().ok_or_else(|| {
            anyhow!(
                "`commands.{command}` holds {}, and every value must hold a string",
                article(spec.type_str())
            )
        })?;
        // The key stays exactly as the file spells it.
        mappings.insert(command.clone(), spec.to_string());
    }
    Ok(mappings)
}

/// Build the `Config` from an explicit file, or from a list of paths.
///
/// `config_file` replaces `paths`. An explicit file must exist. A path in
/// `paths` need not exist, and an absent path adds no entry.
fn load_from(config_file: Option<PathBuf>, paths: &[PathBuf]) -> anyhow::Result<Config> {
    let mut config = Config::default();

    if let Some(path) = config_file {
        // uv reports an error when `UV_CONFIG_FILE` names an absent file.
        // `uvxy` reports an error too.
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read `{}`", path.display()))?;
        merge(&mut config, &path, &text)?;
        return Ok(config);
    }

    for path in paths {
        if let Some(text) = read_optional(path)? {
            merge(&mut config, path, &text)?;
        }
    }
    Ok(config)
}

/// Merge the mappings of one file into `config`. The new file wins.
fn merge(config: &mut Config, path: &Path, text: &str) -> anyhow::Result<()> {
    let mappings = parse(text).with_context(|| format!("cannot parse `{}`", path.display()))?;
    config.files_read.push(path.to_path_buf());
    for (command, spec) in mappings {
        config.sources.insert(command.clone(), path.to_path_buf());
        config.mappings.insert(command, spec);
    }
    Ok(())
}

/// Read the text of one file. Return `None` when the file is absent.
fn read_optional(path: &Path) -> anyhow::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("cannot read `{}`", path.display())),
    }
}

/// Return the value of one environment variable. An empty value counts as no
/// value, because uv treats an empty `XDG_CONFIG_HOME` as no value.
fn env_value(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Return the paths for Unix, and for macOS.
#[cfg(not(windows))]
fn unix_paths(xdg_config_home: Option<&str>, home: Option<&str>) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(SYSTEM_DIR).join(FILE_NAME)];
    let user_dir = match xdg_config_home {
        Some(dir) => Some(PathBuf::from(dir)),
        // `XDG_CONFIG_HOME` is empty, so the default directory applies.
        None => home.map(|dir| PathBuf::from(dir).join(".config")),
    };
    if let Some(user_dir) = user_dir {
        paths.push(user_dir.join("uv").join(FILE_NAME));
    }
    paths
}

/// Return the paths for Windows. Windows has no system file.
#[cfg(windows)]
fn windows_paths(appdata: Option<&str>) -> Vec<PathBuf> {
    match appdata {
        Some(dir) => vec![PathBuf::from(dir).join("uv").join(FILE_NAME)],
        None => Vec::new(),
    }
}

/// Prefix a TOML type name with the correct article.
fn article(type_name: &str) -> String {
    let first = type_name.chars().next().unwrap_or('x');
    if "aeiou".contains(first) {
        format!("an {type_name}")
    } else {
        format!("a {type_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Mappings` from pairs, so a test reads in one line.
    fn mappings(pairs: &[(&str, &str)]) -> Mappings {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// Write `text` to `dir/name`, and return the path.
    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn parse_reads_every_mapping() {
        let text = r#"
            [commands]
            sphinx-build = "sphinx"
            aws = "awscli"
            ansible-playbook = "ansible-core>=2.16"
        "#;
        assert_eq!(
            parse(text).unwrap(),
            mappings(&[
                ("ansible-playbook", "ansible-core>=2.16"),
                ("aws", "awscli"),
                ("sphinx-build", "sphinx"),
            ])
        );
    }

    #[test]
    fn parse_accepts_empty_text() {
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn parse_accepts_a_file_that_holds_only_comments() {
        let text = "# uvxy reads no mapping from this file.\n";
        assert!(parse(text).unwrap().is_empty());
    }

    #[test]
    fn parse_accepts_an_empty_commands_table() {
        assert!(parse("[commands]\n").unwrap().is_empty());
    }

    #[test]
    fn parse_keeps_a_key_exactly() {
        // A normalizing reader would merge these three keys into one.
        let text = r#"
            [commands]
            Sphinx_Build = "a"
            sphinx-build = "b"
            "sphinx.build" = "c"
        "#;
        let result = parse(text).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result["Sphinx_Build"], "a");
        assert_eq!(result["sphinx-build"], "b");
        assert_eq!(result["sphinx.build"], "c");
    }

    #[test]
    fn parse_rejects_an_unknown_table() {
        let err = parse("[from]\nsphinx-build = \"sphinx\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("`from`"), "{err}");
        assert!(err.contains("`commands`"), "{err}");
    }

    #[test]
    fn parse_rejects_an_unknown_key_beside_a_good_table() {
        let text = "[commands]\naws = \"awscli\"\n\n[extra]\nkey = 1\n";
        let err = parse(text).unwrap_err().to_string();
        assert!(err.contains("`extra`"), "{err}");
    }

    #[test]
    fn parse_rejects_a_wrong_case_table() {
        let err = parse("[Commands]\naws = \"awscli\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("`Commands`"), "{err}");
    }

    #[test]
    fn parse_rejects_a_commands_table_that_is_not_a_table() {
        let err = parse("commands = \"sphinx\"\n").unwrap_err().to_string();
        assert!(err.contains("`commands`"), "{err}");
        assert!(err.contains("table"), "{err}");
    }

    #[test]
    fn parse_rejects_a_value_that_is_not_a_string() {
        for text in [
            "[commands]\nsphinx-build = 3\n",
            "[commands]\nsphinx-build = true\n",
            "[commands]\nsphinx-build = [\"sphinx\"]\n",
            "[commands]\n[commands.sphinx-build]\nname = \"sphinx\"\n",
        ] {
            let err = parse(text).unwrap_err().to_string();
            assert!(err.contains("commands.sphinx-build"), "{err}");
            assert!(err.contains("string"), "{err}");
        }
    }

    #[test]
    fn parse_rejects_malformed_text() {
        for text in [
            "[from\nsphinx-build = \"sphinx\"\n",
            "this is not toml [[[\n",
            "[commands]\nsphinx-build =\n",
            "[commands]\nsphinx-build = \"sphinx\"\n[commands]\naws = \"awscli\"\n",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn load_from_returns_an_empty_config_when_no_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let paths = vec![dir.path().join("system.toml"), dir.path().join("user.toml")];
        let config = load_from(None, &paths).unwrap();
        assert!(config.mappings.is_empty());
        assert!(config.sources.is_empty());
        assert!(config.files_read.is_empty());
    }

    #[test]
    fn load_from_merges_the_user_file_over_the_system_file() {
        let dir = tempfile::tempdir().unwrap();
        let system = write(
            dir.path(),
            "system.toml",
            "[commands]\naws = \"awscli\"\nsphinx-build = \"sphinx==7\"\n",
        );
        let user = write(
            dir.path(),
            "user.toml",
            "[commands]\nsphinx-build = \"sphinx==8\"\npygmentize = \"pygments\"\n",
        );

        let config = load_from(None, &[system.clone(), user.clone()]).unwrap();

        assert_eq!(
            config.mappings,
            mappings(&[
                ("aws", "awscli"),
                ("pygmentize", "pygments"),
                ("sphinx-build", "sphinx==8"),
            ])
        );
        // The user file supplied the winning value.
        assert_eq!(config.sources["sphinx-build"], user);
        assert_eq!(config.sources["aws"], system);
        assert_eq!(config.sources["pygmentize"], user);
        // The read order runs from the system file to the user file.
        assert_eq!(config.files_read, vec![system, user]);
    }

    #[test]
    fn load_from_skips_an_absent_path() {
        let dir = tempfile::tempdir().unwrap();
        let user = write(dir.path(), "user.toml", "[commands]\naws = \"awscli\"\n");
        let absent = dir.path().join("system.toml");

        let config = load_from(None, &[absent, user.clone()]).unwrap();

        assert_eq!(config.mappings, mappings(&[("aws", "awscli")]));
        assert_eq!(config.files_read, vec![user]);
    }

    #[test]
    fn load_from_names_the_file_that_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let bad = write(dir.path(), "user.toml", "[commands]\naws = 3\n");

        let err = load_from(None, std::slice::from_ref(&bad)).unwrap_err();
        let text = format!("{err:#}");

        assert!(text.contains(&bad.display().to_string()), "{text}");
        assert!(text.contains("string"), "{text}");
    }

    #[test]
    fn load_from_replaces_every_path_with_the_explicit_file() {
        let dir = tempfile::tempdir().unwrap();
        let system = write(dir.path(), "system.toml", "[commands]\naws = \"awscli\"\n");
        let user = write(dir.path(), "user.toml", "[commands]\naws = \"awscli-v2\"\n");
        let explicit = write(
            dir.path(),
            "explicit.toml",
            "[commands]\nblack = \"black\"\n",
        );

        let config = load_from(Some(explicit.clone()), &[system, user]).unwrap();

        assert_eq!(config.mappings, mappings(&[("black", "black")]));
        assert_eq!(config.sources["black"], explicit);
        assert_eq!(config.files_read, vec![explicit]);
    }

    #[test]
    fn load_from_rejects_an_explicit_file_that_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("explicit.toml");

        let err = load_from(Some(absent.clone()), &[]).unwrap_err();
        let text = format!("{err:#}");

        assert!(text.contains(&absent.display().to_string()), "{text}");
    }

    #[test]
    fn load_from_rejects_an_explicit_file_that_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let bad = write(dir.path(), "explicit.toml", "[from\n");

        let err = load_from(Some(bad.clone()), &[]).unwrap_err();
        let text = format!("{err:#}");

        assert!(text.contains(&bad.display().to_string()), "{text}");
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_paths_prefer_xdg_config_home() {
        assert_eq!(
            unix_paths(Some("/x/cfg"), Some("/home/u")),
            vec![
                PathBuf::from("/etc/uv/uvxy.toml"),
                PathBuf::from("/x/cfg/uv/uvxy.toml"),
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_paths_revert_to_the_home_directory() {
        assert_eq!(
            unix_paths(None, Some("/home/u")),
            vec![
                PathBuf::from("/etc/uv/uvxy.toml"),
                PathBuf::from("/home/u/.config/uv/uvxy.toml"),
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_paths_return_the_system_path_alone() {
        assert_eq!(
            unix_paths(None, None),
            vec![PathBuf::from("/etc/uv/uvxy.toml")]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_read_appdata() {
        assert_eq!(
            windows_paths(Some(r"C:\Users\u\AppData\Roaming")),
            vec![PathBuf::from(r"C:\Users\u\AppData\Roaming\uv\uvxy.toml")]
        );
        assert!(windows_paths(None).is_empty());
    }

    #[test]
    fn config_paths_return_the_system_path_first() {
        let paths = config_paths();
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|p| p.ends_with(FILE_NAME)));
        #[cfg(not(windows))]
        assert_eq!(paths[0], PathBuf::from("/etc/uv/uvxy.toml"));
    }

    #[test]
    fn article_reads_the_first_letter() {
        assert_eq!(article("integer"), "an integer");
        assert_eq!(article("string"), "a string");
    }
}
