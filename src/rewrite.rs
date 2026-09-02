//! Find the command, and build the arguments for uv.
//!
//! This module reads no file and starts no process. The tests target it.
//!
//! See `docs/adr/0003-drop-in-replacement-and-flag-namespace.md`.

use crate::Mappings;
use crate::flags::FlagTable;

/// The mapping that `uvxy` used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The command that the user typed, without any `@version` part.
    pub command: String,
    /// The value that `uvxy` gave to `--from`.
    pub spec: String,
}

/// What `uvxy` will do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// The arguments for `uv tool run`. This list excludes `tool` and `run`.
    pub uv_args: Vec<String>,
    /// The `--uvxy-` flags that `uvxy` removed.
    pub uvxy_flags: Vec<String>,
    /// The command, when `uvxy` found one.
    pub command: Option<String>,
    /// The mapping, when `uvxy` applied one.
    pub applied: Option<Applied>,
}

/// The prefix of a namespaced flag. See `CONTEXT.md`.
const NAMESPACE: &str = "--uvxy-";

/// The separator that ends the uv flags.
const SEPARATOR: &str = "--";

/// The characters that mark a version constraint inside a package spec.
const CONSTRAINT_CHARS: &str = "=<>!~@";

/// Build the arguments for uv.
///
/// `table` holds the uv flags that take a value. A `None` table means that
/// `uvxy` could not read the flags. Read the first argument as the command in
/// that case.
///
/// The rules:
///
/// 1. Read the arguments from left to right.
/// 2. Remove an argument that starts with `--uvxy-`. Record it in
///    `uvxy_flags`. Never send it to uv.
/// 3. Skip a flag. Skip the argument after it when the table says that the
///    flag takes a value. A `--flag=value` argument carries its own value.
/// 4. Stop at `--`. The argument after `--` is the command.
/// 5. The first argument that is neither a flag nor a flag value is the
///    command.
/// 6. Copy every argument after the command without a change.
/// 7. Insert `--from <spec>` directly before the command. Insert it before a
///    `--` separator when one is present.
/// 8. Insert nothing when the user already passed `--from`.
/// 9. Split a command that reads `name@version`. Look up `name`. Build
///    `--from <spec>@<version>`, and pass `name` as the command.
/// 10. Return an error when the spec already holds a version and the user also
///     passed `@version`.
pub fn rewrite(
    args: &[String],
    table: Option<&FlagTable>,
    mappings: &Mappings,
) -> anyhow::Result<Plan> {
    let scan = scan(args, table);

    let mut uv_args = scan.uv_args;
    let mut plan = Plan {
        uvxy_flags: scan.uvxy_flags,
        ..Plan::default()
    };

    // Rule 5. `uvxy` changes nothing when it finds no command.
    let Some(command_at) = scan.command_at else {
        plan.uv_args = uv_args;
        return Ok(plan);
    };

    // Rule 9. Read the `@version` part that the user typed.
    let typed = uv_args[command_at].clone();
    let (name, version) = split_version(&typed);
    plan.command = Some(name.to_string());

    // Rule 8. The user owns the `--from` value when the user gives one.
    if scan.user_from {
        plan.uv_args = uv_args;
        return Ok(plan);
    }

    // A command that no mapping names reaches uv without a change. uv then
    // reads the `@version` part itself.
    let Some(spec) = mappings.get(name) else {
        plan.uv_args = uv_args;
        return Ok(plan);
    };

    let spec = match version {
        None => spec.clone(),
        Some(version) => {
            // Rule 10. Two versions for one package contradict each other.
            if spec.contains(|c| CONSTRAINT_CHARS.contains(c)) {
                anyhow::bail!(
                    "the mapping for `{name}` holds the version pin `{spec}`, \
                     and the command holds the version pin `@{version}`. \
                     Remove one of the two pins."
                );
            }
            // uv accepts the `@version` form after a package name.
            // Rule 9 sends the bare name as the command.
            uv_args[command_at] = name.to_string();
            format!("{spec}@{version}")
        }
    };

    // Rule 7. The insertion point sits before a `--` separator, because every
    // argument after `--` belongs to the command.
    uv_args.insert(scan.insert_at, spec.clone());
    uv_args.insert(scan.insert_at, "--from".to_string());

    plan.applied = Some(Applied {
        command: name.to_string(),
        spec,
    });
    plan.uv_args = uv_args;
    Ok(plan)
}

/// The result of one left to right read of the arguments.
struct Scan {
    /// Every argument except a namespaced flag.
    uv_args: Vec<String>,
    /// The namespaced flags, in the order that the user typed them.
    uvxy_flags: Vec<String>,
    /// The index of the command inside `uv_args`.
    command_at: Option<usize>,
    /// The index inside `uv_args` that receives `--from`.
    insert_at: usize,
    /// True when the user already passed `--from`.
    user_from: bool,
}

/// Read the arguments from left to right, and find the command.
fn scan(args: &[String], table: Option<&FlagTable>) -> Scan {
    let mut out = Scan {
        uv_args: Vec::with_capacity(args.len()),
        uvxy_flags: Vec::new(),
        command_at: None,
        insert_at: 0,
        user_from: false,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Rule 4. uv consumes the `--` itself, so the output keeps it.
        if arg == SEPARATOR {
            out.insert_at = out.uv_args.len();
            out.uv_args.push(arg.clone());
            i += 1;
            if i < args.len() {
                out.command_at = Some(out.uv_args.len());
                out.uv_args.extend_from_slice(&args[i..]);
            }
            return out;
        }

        // Rule 2. A namespaced flag reaches uv never. The scan reads it only
        // before the command and before a `--` separator.
        if arg.starts_with(NAMESPACE) {
            out.uvxy_flags.push(arg.clone());
            i += 1;
            continue;
        }

        if is_flag(arg) {
            // The `None` table gives no arity, so the scan stops here and
            // finds no command. See ADR 0002.
            let Some(table) = table else {
                out.uv_args.extend_from_slice(&args[i..]);
                return out;
            };
            if arg == "--from" || arg.starts_with("--from=") {
                out.user_from = true;
            }
            out.uv_args.push(arg.clone());
            i += 1;
            // Rule 3. A `--flag=value` argument holds its own value, so the
            // argument after it stays free.
            if !carries_value(arg) && table.takes_value(arg) && i < args.len() {
                out.uv_args.push(args[i].clone());
                i += 1;
            }
            continue;
        }

        // Rule 5 and rule 6. Every argument after the command belongs to the
        // command, so the scan ends here.
        out.insert_at = out.uv_args.len();
        out.command_at = Some(out.uv_args.len());
        out.uv_args.extend_from_slice(&args[i..]);
        return out;
    }

    out
}

/// Report whether the argument reads as a flag.
///
/// The caller handles `--` before this test.
fn is_flag(arg: &str) -> bool {
    arg.starts_with('-') && arg.len() > 1
}

/// Report whether a long flag holds its own value, as in `--python=3.12`.
fn carries_value(arg: &str) -> bool {
    arg.starts_with("--") && arg.contains('=')
}

/// Split `name@version` at the first `@`.
///
/// Return the whole argument and `None` when the argument holds no version.
/// A leading `@` and a trailing `@` name no version.
fn split_version(arg: &str) -> (&str, Option<&str>) {
    match arg.find('@') {
        Some(at) if at > 0 && at + 1 < arg.len() => (&arg[..at], Some(&arg[at + 1..])),
        _ => (arg, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flags that take a value in these tests.
    fn table() -> FlagTable {
        FlagTable::from_flags([
            "--with",
            "-w",
            "--from",
            "--python",
            "-p",
            "--with-requirements",
            "-c",
        ])
    }

    fn mappings() -> Mappings {
        [
            ("sphinx-build", "sphinx"),
            ("mytool", "acme-mytool"),
            ("pinned", "acme-pinned==2.1"),
            ("black", "black-nightly"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    fn owned(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// One test case. `use_table` false gives the fallback of ADR 0002.
    struct Case {
        name: &'static str,
        args: &'static [&'static str],
        use_table: bool,
        uv_args: &'static [&'static str],
        uvxy_flags: &'static [&'static str],
        command: Option<&'static str>,
        applied: Option<(&'static str, &'static str)>,
    }

    const CASES: &[Case] = &[
        Case {
            name: "a flag value never reads as the command",
            args: &["--with", "sphinx-rtd-theme", "sphinx-build", "docs", "out"],
            use_table: true,
            uv_args: &[
                "--with",
                "sphinx-rtd-theme",
                "--from",
                "sphinx",
                "sphinx-build",
                "docs",
                "out",
            ],
            uvxy_flags: &[],
            command: Some("sphinx-build"),
            applied: Some(("sphinx-build", "sphinx")),
        },
        Case {
            name: "the second black is the command",
            args: &["--with", "black", "black"],
            use_table: true,
            uv_args: &["--with", "black", "--from", "black-nightly", "black"],
            uvxy_flags: &[],
            command: Some("black"),
            applied: Some(("black", "black-nightly")),
        },
        Case {
            name: "an argument after the command stays untouched",
            args: &["sphinx-build", "--with", "foo"],
            use_table: true,
            uv_args: &["--from", "sphinx", "sphinx-build", "--with", "foo"],
            uvxy_flags: &[],
            command: Some("sphinx-build"),
            applied: Some(("sphinx-build", "sphinx")),
        },
        Case {
            name: "a user --from wins",
            args: &["--from", "x", "sphinx-build"],
            use_table: true,
            uv_args: &["--from", "x", "sphinx-build"],
            uvxy_flags: &[],
            command: Some("sphinx-build"),
            applied: None,
        },
        Case {
            name: "a user --from=value wins",
            args: &["--from=x", "sphinx-build"],
            use_table: true,
            uv_args: &["--from=x", "sphinx-build"],
            uvxy_flags: &[],
            command: Some("sphinx-build"),
            applied: None,
        },
        Case {
            name: "--from goes before the separator",
            args: &["--", "sphinx-build"],
            use_table: true,
            uv_args: &["--from", "sphinx", "--", "sphinx-build"],
            uvxy_flags: &[],
            command: Some("sphinx-build"),
            applied: Some(("sphinx-build", "sphinx")),
        },
        Case {
            name: "the separator ends the namespace",
            args: &["--", "--uvxy-thing"],
            use_table: true,
            uv_args: &["--", "--uvxy-thing"],
            uvxy_flags: &[],
            command: Some("--uvxy-thing"),
            applied: None,
        },
        Case {
            name: "a namespaced flag reaches uv never",
            args: &["--uvxy-explain", "mytool"],
            use_table: true,
            uv_args: &["--from", "acme-mytool", "mytool"],
            uvxy_flags: &["--uvxy-explain"],
            command: Some("mytool"),
            applied: Some(("mytool", "acme-mytool")),
        },
        Case {
            name: "the version moves to the spec",
            args: &["mytool@1.2"],
            use_table: true,
            uv_args: &["--from", "acme-mytool@1.2", "mytool"],
            uvxy_flags: &[],
            command: Some("mytool"),
            applied: Some(("mytool", "acme-mytool@1.2")),
        },
        Case {
            name: "a flag that holds its own value frees the next argument",
            args: &["--python=3.12", "mytool"],
            use_table: true,
            uv_args: &["--python=3.12", "--from", "acme-mytool", "mytool"],
            uvxy_flags: &[],
            command: Some("mytool"),
            applied: Some(("mytool", "acme-mytool")),
        },
        Case {
            name: "a command that no mapping names stays unchanged",
            args: &["--with", "foo", "othertool", "-x"],
            use_table: true,
            uv_args: &["--with", "foo", "othertool", "-x"],
            uvxy_flags: &[],
            command: Some("othertool"),
            applied: None,
        },
        Case {
            name: "no argument gives no command",
            args: &[],
            use_table: true,
            uv_args: &[],
            uvxy_flags: &[],
            command: None,
            applied: None,
        },
        Case {
            name: "no table and a leading flag gives no command",
            args: &["--with", "foo", "mytool"],
            use_table: false,
            uv_args: &["--with", "foo", "mytool"],
            uvxy_flags: &[],
            command: None,
            applied: None,
        },
        Case {
            name: "no table reads the first argument as the command",
            args: &["mytool", "arg"],
            use_table: false,
            uv_args: &["--from", "acme-mytool", "mytool", "arg"],
            uvxy_flags: &[],
            command: Some("mytool"),
            applied: Some(("mytool", "acme-mytool")),
        },
    ];

    #[test]
    fn cases() {
        let table = table();
        let mappings = mappings();
        for case in CASES {
            let table = if case.use_table { Some(&table) } else { None };
            let plan = rewrite(&owned(case.args), table, &mappings)
                .unwrap_or_else(|err| panic!("{}: {err}", case.name));
            assert_eq!(plan.uv_args, owned(case.uv_args), "uv_args: {}", case.name);
            assert_eq!(
                plan.uvxy_flags,
                owned(case.uvxy_flags),
                "uvxy_flags: {}",
                case.name
            );
            assert_eq!(
                plan.command.as_deref(),
                case.command,
                "command: {}",
                case.name
            );
            let applied = case.applied.map(|(command, spec)| Applied {
                command: command.to_string(),
                spec: spec.to_string(),
            });
            assert_eq!(plan.applied, applied, "applied: {}", case.name);
        }
    }

    #[test]
    fn two_pins_give_an_error() {
        let err = rewrite(&owned(&["pinned@1.2"]), Some(&table()), &mappings())
            .expect_err("two pins must give an error");
        let message = format!("{err}");
        assert!(message.contains("acme-pinned==2.1"), "{message}");
        assert!(message.contains("1.2"), "{message}");
        assert!(message.contains("pinned"), "{message}");
    }

    #[test]
    fn a_pinned_spec_without_a_typed_version_works() {
        let plan = rewrite(&owned(&["pinned", "run"]), Some(&table()), &mappings()).unwrap();
        assert_eq!(
            plan.uv_args,
            owned(&["--from", "acme-pinned==2.1", "pinned", "run"])
        );
    }

    #[test]
    fn a_separator_at_the_end_gives_no_command() {
        let plan = rewrite(
            &owned(&["--with", "foo", "--"]),
            Some(&table()),
            &mappings(),
        )
        .unwrap();
        assert_eq!(plan.uv_args, owned(&["--with", "foo", "--"]));
        assert_eq!(plan.command, None);
        assert_eq!(plan.applied, None);
    }

    #[test]
    fn a_short_flag_takes_its_value() {
        let plan = rewrite(
            &owned(&["-p", "3.12", "-w", "extra", "mytool"]),
            Some(&table()),
            &mappings(),
        )
        .unwrap();
        assert_eq!(
            plan.uv_args,
            owned(&[
                "-p",
                "3.12",
                "-w",
                "extra",
                "--from",
                "acme-mytool",
                "mytool"
            ])
        );
    }

    #[test]
    fn a_namespaced_flag_after_the_command_reaches_the_command() {
        let plan = rewrite(
            &owned(&["mytool", "--uvxy-foo"]),
            Some(&table()),
            &mappings(),
        )
        .unwrap();
        assert_eq!(
            plan.uv_args,
            owned(&["--from", "acme-mytool", "mytool", "--uvxy-foo"])
        );
        assert!(plan.uvxy_flags.is_empty());
    }

    #[test]
    fn a_namespaced_flag_holds_its_order() {
        let plan = rewrite(
            &owned(&["--uvxy-explain", "--uvxy-version", "--with", "x", "mytool"]),
            Some(&table()),
            &mappings(),
        )
        .unwrap();
        assert_eq!(
            plan.uvxy_flags,
            owned(&["--uvxy-explain", "--uvxy-version"])
        );
        assert_eq!(
            plan.uv_args,
            owned(&["--with", "x", "--from", "acme-mytool", "mytool"])
        );
    }

    #[test]
    fn no_table_removes_a_leading_namespaced_flag() {
        let plan = rewrite(&owned(&["--uvxy-explain", "mytool"]), None, &mappings()).unwrap();
        assert_eq!(plan.uvxy_flags, owned(&["--uvxy-explain"]));
        assert_eq!(plan.uv_args, owned(&["--from", "acme-mytool", "mytool"]));
    }

    #[test]
    fn no_table_keeps_the_separator_rule() {
        let plan = rewrite(&owned(&["--", "mytool"]), None, &mappings()).unwrap();
        assert_eq!(
            plan.uv_args,
            owned(&["--from", "acme-mytool", "--", "mytool"])
        );
        assert_eq!(plan.command.as_deref(), Some("mytool"));
    }

    #[test]
    fn a_version_after_the_command_reaches_the_command() {
        let plan = rewrite(
            &owned(&["mytool", "other@2.0"]),
            Some(&table()),
            &mappings(),
        )
        .unwrap();
        assert_eq!(
            plan.uv_args,
            owned(&["--from", "acme-mytool", "mytool", "other@2.0"])
        );
    }

    #[test]
    fn an_unmapped_command_keeps_its_version() {
        let plan = rewrite(&owned(&["othertool@1.2"]), Some(&table()), &mappings()).unwrap();
        assert_eq!(plan.uv_args, owned(&["othertool@1.2"]));
        assert_eq!(plan.command.as_deref(), Some("othertool"));
        assert_eq!(plan.applied, None);
    }
}
