# Development plan

This plan builds `uvxy` in four phases. Phase 1 and phase 2 run in parallel.
Each parallel task owns one file, so no two tasks edit the same file.

The decisions behind this plan live in `docs/adr/`. The terms live in
`CONTEXT.md`.

## Phase 0 — Scaffold

One task, and it must finish first. It fixes the interfaces that phase 1 fills
in.

- Create `Cargo.toml` with every dependency the project needs. Later tasks must
  never edit this file.
- Create `pyproject.toml` for maturin, with `bindings = "bin"`.
- Create `src/main.rs`. Declare each module. Define the shared types and the
  function signature of each module entry point. Give each function a
  `todo!()` body.
- Create `.gitignore`.

## Phase 1 — Modules

Four tasks run in parallel. Each task owns one file.

| File            | Task                                                          |
|-----------------|---------------------------------------------------------------|
| `src/config.rs` | Find `uvxy.toml`. Read `[commands]`. Merge system under user.      |
| `src/flags.rs`  | Read arity from `uv generate-shell-completion zsh`. Cache it.  |
| `src/rewrite.rs`| Find the command. Insert `--from`. Handle `@version` and `--`. |
| `src/uvbin.rs`  | Find the `uv` binary. Replace the process with it.             |

### src/config.rs

- Read `$UVXY_CONFIG_FILE` if the variable is set. That file replaces every
  other file.
- Otherwise read the system file, then the user file. Merge them for each
  command name. The user file wins.
- Paths follow uv. Use `$XDG_CONFIG_HOME/uv/uvxy.toml`, `/etc/uv/uvxy.toml`,
  and `%APPDATA%\uv\uvxy.toml`.
- Read the `[commands]` table. Each key is a command. Each value is a package spec
  string.
- Match a key exactly. Do not normalize the key.
- Exit with an error if a file exists and does not parse.

### src/flags.rs

- Run `uv generate-shell-completion zsh`.
- Read the arity of each `uv tool run` flag. A flag that takes a value carries
  a `:NAME:` marker, as in `'--from=[...]:FROM:_default'`.
- Cache the result in `$XDG_CACHE_HOME/uvxy/`. On Windows use
  `%LOCALAPPDATA%\uvxy\cache`.
- Key the cache on the path, the modification time, and the size of the `uv`
  binary. Read that key with one `stat` call.
- Write the cache to a temporary file in the same directory. Then rename it.
  Two `uvxy` processes must not tear the file.
- Do not fail if the cache directory refuses writes. Derive the table again on
  the next run.
- Return an error if the output no longer yields a table. The caller then warns
  and falls back.

### src/rewrite.rs

This module holds the logic that the tests target. It performs no input and no
output.

Input: the arguments, the flag table, and the mappings. Output: the arguments
for `uv tool run`.

1. Read arguments from left to right.
2. Consume a flag that starts with `--uvxy-`. Never send it to uv.
3. Skip a flag. Skip its value if the flag table says the flag takes one. A
   `--flag=value` argument carries its own value.
4. Stop at `--`. The next argument is the command.
5. The first argument that is not a flag and not a flag value is the command.
6. Send every argument after the command to uv without a change.
7. Look up the command. Insert `--from <spec>` directly before the command, and
   before a `--` separator.
8. Insert nothing if the user already passed `--from`.
9. Split a command of the form `name@version`. Look up `name`. Build
   `--from <spec>@<version>` and pass `name` as the command.
10. Exit with an error if the spec already carries a version and the user also
    passed `@version`.

### src/uvbin.rs

- Read `$UVXY_UV` first. Then read `$UV`. Then search `PATH` for `uv`.
- Run `uv tool run`. Use the same binary that produced the flag table.
- Warn and fall back to `uvx` if `uv` is absent and `uvx` is present. Use the
  first argument as the command in that case.
- Replace the process with `execvp` on Unix. Windows has no `exec`. Start a
  child process there and return its exit code.

## Phase 2 — Tests, CI, and documentation

Three tasks run in parallel with phase 1. They depend only on the interfaces
from phase 0.

| File                     | Task                                     |
|--------------------------|------------------------------------------|
| `tests/`                 | Table tests, and tests of the whole binary |
| `.github/workflows/`     | Build, test, drift check, and publish      |
| `README.md`              | Usage, and an example configuration file   |

### Tests

- Write table tests for `rewrite`. Each case gives arguments, a flag table, and
  mappings. Each case asserts the output arguments.
- Cover these cases:
  - `uvxy --with sphinx-rtd-theme sphinx-build docs out`
  - `uvxy --with black black`
  - `uvxy sphinx-build --with foo`
  - `uvxy --from x sphinx-build`
  - `uvxy -- sphinx-build`
  - `uvxy -- --uvxy-thing`
  - `uvxy mytool@1.2` with an unpinned spec, and with a pinned spec
  - `uvxy --python=3.12 mytool`
  - a command that no mapping names
- Write tests that run the built binary. Put a fake `uv` on `PATH`. The fake
  writes its arguments to a file. Assert on that file.
- Write one test that reads the real `uv` output and checks that arity still
  parses. Skip that test when `uv` is absent.

### CI

- Run `cargo test`, `cargo clippy`, and `cargo fmt --check` on every push.
- Run the drift check against the current uv release each day. A failure here
  reports that uv changed its completion format.
- Build seven wheels and one sdist with `PyO3/maturin-action`.
- Publish to PyPI on a tag. Use trusted publishing.

### README

- State the problem. `uvx` infers a package name from the command.
- Show the configuration file and its location.
- Give an example `[commands]` table. Include `aws`, `sphinx-build`,
  `ansible-playbook`, and `pygmentize`.
- List the `--uvxy-` flags.
- State that `uvxy` accepts every `uvx` argument.

## Phase 3 — Integration

One task, and it runs last.

1. Run `cargo build` and `cargo test`.
2. Run the binary against a fake `uv` and check the arguments.
3. Build a wheel with `uvx maturin build`.
4. Install that wheel and run `uvxy --uvxy-explain` against a real command.
