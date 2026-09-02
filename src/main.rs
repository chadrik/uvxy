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

uvxy reads mappings from the [from] table of uvxy.toml:

  [from]
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

    // A missing uv binary is fatal.
    let uv = uvbin::resolve()?;

    // A malformed configuration file is fatal. See ADR 0001.
    let config = config::load()?;

    // A flag table that does not build is not fatal. See ADR 0002.
    let table = match flags::load(&uv) {
        Ok(table) => Some(table),
        Err(err) => {
            eprintln!("uvxy: warning: cannot read the uv flags: {err:#}");
            eprintln!("uvxy: warning: uvxy reads the first argument as the command");
            None
        }
    };

    let plan = rewrite::rewrite(&args, table.as_ref(), &config.mappings)?;

    for flag in &plan.uvxy_flags {
        match flag.as_str() {
            "--uvxy-help" => {
                print!("{HELP}");
                return Ok(0);
            }
            "--uvxy-version" => {
                println!("uvxy {VERSION}");
                return Ok(0);
            }
            "--uvxy-explain" => {
                explain(&uv, &plan, &config);
                return Ok(0);
            }
            other => anyhow::bail!("unknown flag `{other}`. Run `uvxy --uvxy-help`."),
        }
    }

    uvbin::exec(&uv, &plan.uv_args)
}

/// Print the command that `uvxy` would run, and where the mapping came from.
fn explain(uv: &uvbin::Uv, plan: &rewrite::Plan, config: &config::Config) {
    let mut line: Vec<String> = vec![uv.path.display().to_string()];
    line.extend(uv.prefix_args().iter().map(|s| s.to_string()));
    line.extend(plan.uv_args.iter().cloned());
    println!("{}", shell_quote(&line));

    match &plan.command {
        Some(command) => println!("command: {command}"),
        None => println!("command: none found"),
    }

    match &plan.applied {
        Some(applied) => {
            println!("mapping: {} = \"{}\"", applied.command, applied.spec);
            match config.sources.get(&applied.command) {
                Some(path) => println!("source:  {}", path.display()),
                None => println!("source:  unknown"),
            }
        }
        None => println!("mapping: none applied"),
    }

    if config.files_read.is_empty() {
        println!("config:  no file found");
    } else {
        for path in &config.files_read {
            println!("config:  {}", path.display());
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
