# Store mappings in uvxy.toml, and do not search the working directory

`uvxy` reads its mappings from `uvxy.toml`. `uvxy` looks for that file in the
same directories that uv uses for user configuration and system configuration.
`uvxy` does not search the working directory or any parent directory.

## Why a separate file

uv validates `uv.toml` against a closed list of keys. A `[uvxy]` table in
`uv.toml` produces this error:

```
error: Failed to parse: `uv.toml`
  Caused by: TOML parse error at line 1, column 2
  |
1 | [uvxy]
  |  ^^^^
unknown field `uvxy`, expected one of `required-version`, `native-tls`, ...
```

`uv.toml` has no `[tool.*]` extension point. That point exists only in
`pyproject.toml`. A `[uvxy]` table in the user configuration file therefore
breaks every uv command on the machine. It also breaks the `uv tool run` that
`uvxy` calls. The configuration would disable the tool that reads it.

`uvxy.toml` sits next to `uv.toml` in the same directory. One directory holds
both files.

## Why no directory search

`uv tool run` ignores `uv.toml` and `pyproject.toml` in the working directory
and in parent directories. It reads only user configuration, system
configuration, and `UV_CONFIG_FILE`. A tool run is not scoped to a project.

`uvxy` copies that behavior. Two results follow:

1. `uvxy mytool` means the same thing in every directory.
2. A repository that you clone cannot redirect `uvxy black` to another package.

## Consequences

- A project cannot pin a private command to a private package. Users who need
  that must set `UVXY_CONFIG_FILE`.
- `uvxy.toml` occupies a filename in a directory that Astral owns.
