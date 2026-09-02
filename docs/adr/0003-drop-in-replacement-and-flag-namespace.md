# uvxy is a drop-in replacement for uvx, and reserves the --uvxy- prefix

`uvxy` accepts every argument that `uvx` accepts, and it means the same thing.
`uvxy` adds no flag to the shared namespace. `uvxy` puts its own flags behind
the `--uvxy-` prefix.

| Input                | Result                                            |
|----------------------|---------------------------------------------------|
| `uvxy --help`        | uvx prints its help                                |
| `uvxy --uvxy-help`   | uvxy prints its own help                           |
| `uvxy --version`     | uvx rejects the flag, as it does today             |
| `uvxy --uvxy-version`| uvxy prints its own version                        |
| `uvxy`               | uvx prints its "provide a command" hint            |

## Why

A user can replace `uvx` with `uvxy` in a shell alias, a script, or a CI job.
Nothing changes, except that a mapped command now gets a `--from`. A reserved
prefix keeps that true in the future. uv will never ship a `--uvxy-` flag, so
`uvxy` can add a flag at any time and shadow nothing.

The rejected alternative was to claim the tokens that uvx does not use today.
`uv tool run` rejects `--version` and `-V`, so `uvxy` could take them. We
rejected this because it trades a permanent guarantee for a temporary gap. uv
could add `--version` in any release, and `uvxy` would then shadow it.

## Scope

`uvxy` replaces `uvx`. It replaces no other uv command.

`uv tool install` needs the same knowledge in reverse. A user who wants
`ansible-playbook` must install `ansible`. The `[commands]` table holds that pair.
We still do not use it there. `uv tool install` takes a package and has no
`--from` flag, so `uvxy` would translate a name rather than synthesize an
argument. That is a different feature. It would also end the guarantee above,
because `uvxy` would then be a front end for uv rather than a replacement for
uvx.

## Consequences

- `uvxy --version` fails. Users must type `uvxy --uvxy-version`.
- `uvxy` scans arguments only up to the command. A `--uvxy-` flag that belongs
  to the command, as in `uvxy mytool --uvxy-foo`, reaches the command
  unchanged.
- `uvxy` stops reading namespaced flags at the command, and also at a `--`
  separator. A user reaches a command whose name starts with `--uvxy-` through
  `uvxy -- --uvxy-thing`. uv consumes the `--` and runs `--uvxy-thing`.
- `uvxy` inserts `--from` before a `--` separator. Everything after `--` belongs
  to the command.
