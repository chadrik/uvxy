# uvxy

`uvxy` runs `uv tool run`, and it supplies the `--from` argument.

## The problem

`uvx` infers a package name from the command you type. Many packages install a
command under a different name. `uvx sphinx-build` therefore fails, because the
package is `sphinx` and the command is `sphinx-build`. You must type the package
name every time:

```console
$ uvx --from sphinx sphinx-build docs build
```

`uvxy` reads that `--from` value from a configuration file:

```console
$ uvxy sphinx-build docs build
```

`uvxy` is a drop-in replacement for `uvx`. It accepts every `uvx` argument, and
each argument means the same thing.

## Install

```console
$ uv tool install uvxy
```

`uvxy` is a Rust binary. PyPI ships it as a wheel for each platform. The wheel
contains no Python code, and `uvxy` needs no Python interpreter at run time.

## Configure

Create `uvxy.toml` in the uv configuration directory:

| Scope       | Path                                                             |
|-------------|------------------------------------------------------------------|
| User        | `$XDG_CONFIG_HOME/uv/uvxy.toml`, default `~/.config/uv/uvxy.toml` |
| User        | `%APPDATA%\uv\uvxy.toml` on Windows                              |
| System      | `/etc/uv/uvxy.toml`                                              |
| Override    | `$UVXY_CONFIG_FILE`                                              |

The user file wins over the system file, one command at a time.
`$UVXY_CONFIG_FILE` replaces every other file.

`uvxy.toml` sits beside `uv.toml` in the same directory, and it is a separate
file. uv validates `uv.toml` against a closed list of keys and rejects an
unknown table, so a `[uvxy]` table in `uv.toml` would break every uv command on
the machine.

`uvxy` does not search the working directory or any parent directory. `uv tool
run` reads the same three sources and ignores the working directory, and `uvxy`
copies that behavior. A command therefore means the same thing in every
directory.

### Example

Copy this table into `uvxy.toml`:

```toml
[commands]
aws = "awscli"
ansible-playbook = "ansible-core"
chardetect = "chardet"
f2py = "numpy"
fab = "fabric"
http = "httpie"
markdown_py = "Markdown"
pip-compile = "pip-tools"
pybabel = "Babel"
pygmentize = "Pygments"
rst2html = "docutils"
sphinx-build = "sphinx"
sqlformat = "sqlparse"
```

`ansible-core` provides the `ansible-playbook` command. The larger `ansible`
package provides the community collections, and it depends on `ansible-core`.
`ansible-playbook = "ansible"` therefore also runs. uv finds the command in the
dependency, and it prints a warning each time. Use `"ansible"` when you need the
collections, and accept that warning.

Each key is a command. Each value is a package spec. A package spec is the value
that `uvxy` gives to `--from`, so it accepts any requirement that `--from`
accepts, including a version:

```toml
[commands]
mytool = "acme-mytool==2.1"
```

`uvxy` matches a key exactly. It does not normalize the key.

## Pin a version on the command line

Write `command@version`. `uvxy` reads the mapping for `command`, and adds the
version to the package spec:

```console
$ uvxy mytool@1.2      # runs: uvx --from acme-mytool@1.2 mytool
```

`uvxy` exits with an error if the package spec already carries a version.

## The `--uvxy-` flags

`uvxy` sends every other argument to uvx without a change, so `uvxy --help`
prints the uvx help. `uvxy` reads its own flags from the `--uvxy-` namespace.
uv will never ship a `--uvxy-` flag, so `uvxy` can add a flag later and shadow
no uv flag.

| Flag              | Result                                                  |
|-------------------|---------------------------------------------------------|
| `--uvxy-help`     | Print the uvxy help                                     |
| `--uvxy-version`  | Print the uvxy version                                  |
| `--uvxy-explain`  | Print the command that uvxy would run, and run nothing  |

Use `--uvxy-explain` to see the result of a mapping:

```console
$ uvxy --uvxy-explain sphinx-build docs build
/usr/local/bin/uv tool run --from sphinx sphinx-build docs build
command: sphinx-build
mapping: sphinx-build = "sphinx"
source:  /home/you/.config/uv/uvxy.toml
config:  /home/you/.config/uv/uvxy.toml
```

`uvxy --version` fails, because uvx rejects `--version` today. Type
`uvxy --uvxy-version` instead.

`uvxy` reads a namespaced flag only before the command. `uvxy mytool --uvxy-foo`
sends `--uvxy-foo` to `mytool`.

## How it works

1. `uvxy` reads the arguments from left to right, and finds the command.
2. `uvxy` reads the mapping for that command from `uvxy.toml`.
3. `uvxy` inserts `--from <spec>` directly before the command.
4. `uvxy` replaces its own process with `uv tool run`.

`uvxy` changes nothing when no mapping names the command, and when you pass
`--from` yourself.

Step 1 needs the arity of each uv flag. `uvxy --with sphinx-rtd-theme
sphinx-build` contains one command, and a rule that takes the first argument
without a leading dash returns `sphinx-rtd-theme`. `uvxy` therefore runs
`uv generate-shell-completion` and reads which flags take a value. It caches
that table under `$XDG_CACHE_HOME/uvxy/`, or under `%LOCALAPPDATA%\uvxy\cache`
on Windows. The cache key is the path, the modification time, and the size of
the uv binary.

## Documentation

- `docs/adr/` records the decisions and the rejected alternatives.
- `CONTEXT.md` defines the terms.

## License

MIT
