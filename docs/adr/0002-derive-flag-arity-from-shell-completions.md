# Derive uvx flag arity from generated shell completions

`uvxy` must find the command in its own arguments. `uvxy --with foo mytool`
contains one command, `mytool`. A rule that takes the first argument without a
leading dash returns `foo`, because `--with` consumes a value.

`uvxy` therefore needs to know which uvx flags take a value. It runs
`uv generate-shell-completion zsh` and reads the arity markers. It caches the
result. The cache key is the path, the modification time, and the size of the
uv binary. A stat call reads that key, so the steady-state cost is near zero.

## Why not read `uv tool run --help`

Help output omits hidden flags. `--generate-shell-completion` takes a value and
appears 0 times in `uv tool run --help`. It appears in the completions. A parser
that reads help output is therefore incomplete today, not only after uv adds a
flag.

## Why not a table inside uvxy

uv 0.8.17 declares 37 value-taking flags on `uv tool run`. A vendored copy of
that list is correct on the day you write it. It then drifts with every uv
release, and `uvxy` gives no signal when it drifts.

## Why not ask uvx to parse the arguments

`uv tool run --show-settings` prints uv's own parse, including `command:` and
`from:`. It exits before it resolves or runs the tool. It cannot disagree with
uv about grammar.

We rejected it for two reasons:

1. It costs 23 ms on every invocation. The completion cache costs about 0 ms.
2. Its output is a Rust `Debug` dump of an internal struct, behind a hidden
   flag. Astral can change it in any release.

## Consequences

- `uvxy` owns a parse loop. That loop must handle `--flag=value`, the `--`
  separator, and repeatable flags.
- If the completion format changes, `uvxy` cannot build a flag table. It then
  writes a warning to stderr and treats the first argument as the command. That
  rule is correct for `uvxy mytool ...`, which is the common invocation. It is
  wrong when a uvx flag precedes the command.
- A malformed `uvxy.toml` is a different class of failure. `uvxy` exits with an
  error in that case, as uv does for a malformed `uv.toml`.
