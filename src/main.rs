//! `uvxy` runs `uv tool run`, and it supplies the `--from` argument.
//!
//! `CONTEXT.md` defines the terms. `docs/adr/` records the decisions.

mod config;
mod flags;
mod rewrite;
mod uvbin;

use std::collections::BTreeMap;

/// A command name, mapped to a package spec.
pub type Mappings = BTreeMap<String, String>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
uvxy runs uv tool run, and it supplies the --from argument.

Usage: uvxy [UVX OPTIONS] <COMMAND> [ARGS]...

uvxy accepts every argument that uvx accepts, and passes it through.
uvxy reads its own flags from the --uvxy- namespace:

  --uvxy-help      Print this message
  --uvxy-version   Print the uvxy version
  --uvxy-explain   Print the command uvxy would run, and do not run it

uvxy reads mappings from the [commands] table of uvxy.toml:

  [commands]
  sphinx-build = \"sphinx\"
";

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code.clamp(0, 255) as u8),
        Err(err) => {
            eprintln!("uvxy: {err:#}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // uvxy holds these two errors rather than returning them. `--uvxy-help`
    // must answer when uv is absent, and when the configuration file is
    // broken. Those are the moments a user asks for help.
    let uv = uvbin::resolve();
    let config = config::load();

    // A flag table that does not build is not fatal. See ADR 0002.
    let table = match &uv {
        Ok(uv) => match flags::load(uv) {
            Ok(table) => Some(table),
            Err(err) => {
                eprintln!("uvxy: warning: cannot read the uv flags: {err:#}");
                eprintln!("uvxy: warning: uvxy reads the first argument as the command");
                None
            }
        },
        Err(_) => None,
    };

    let mappings = match &config {
        Ok(config) => config.mappings.clone(),
        Err(_) => Mappings::new(),
    };
    let plan = rewrite::rewrite(&args, table.as_ref(), &mappings)?;

    // Reject an unknown namespaced flag before uvxy acts on any of them.
    for flag in &plan.uvxy_flags {
        if !matches!(
            flag.as_str(),
            "--uvxy-help" | "--uvxy-version" | "--uvxy-explain"
        ) {
            anyhow::bail!("unknown flag `{flag}`. Run `uvxy --uvxy-help`.");
        }
    }

    if let Some(flag) = plan.uvxy_flags.first() {
        match flag.as_str() {
            "--uvxy-help" => print!("{HELP}"),
            "--uvxy-version" => println!("uvxy {VERSION}"),
            _ => explain(uv.as_ref().ok(), &plan, config.as_ref().ok()),
        }
        return Ok(0);
    }

    // uvxy now needs both. Report the errors that it held.
    let config = config?;
    let _ = config;
    let uv = uv?;
    uvbin::exec(&uv, &plan.uv_args)
}

/// Print the command that `uvxy` would run, and where the mapping came from.
fn explain(uv: Option<&uvbin::Uv>, plan: &rewrite::Plan, config: Option<&config::Config>) {
    match uv {
        Some(uv) => {
            let mut line: Vec<String> = vec![uv.path.display().to_string()];
            line.extend(uv.prefix_args().iter().map(|s| s.to_string()));
            line.extend(plan.uv_args.iter().cloned());
            println!("{}", shell_quote(&line));
        }
        None => println!("uv:      not found"),
    }

    match &plan.command {
        Some(command) => println!("command: {command}"),
        None => println!("command: none found"),
    }

    match &plan.applied {
        Some(applied) => {
            println!("mapping: {} = \"{}\"", applied.command, applied.spec);
            let source = config.and_then(|c| c.sources.get(&applied.command));
            match source {
                Some(path) => println!("source:  {}", path.display()),
                None => println!("source:  unknown"),
            }
        }
        None => println!("mapping: none applied"),
    }

    match config {
        None => println!("config:  cannot read the configuration file"),
        Some(config) if config.files_read.is_empty() => println!("config:  no file found"),
        Some(config) => {
            for path in &config.files_read {
                println!("config:  {}", path.display());
            }
        }
    }
}

/// Join arguments into one line that a shell reads back as the same arguments.
fn shell_quote(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            let safe = !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c));
            if safe {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
