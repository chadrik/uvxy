# uvxy

`uvxy` is a wrapper around `uvx`. `uvx` infers a package name from the command
you type. When the two names differ, you must pass `--from`. `uvxy` reads that
`--from` value from a configuration file instead.

## Language

**Command**:
The executable name that a Python package installs, and the name a user types.
_Avoid_: tool name, script, entry point

**Package Spec**:
The value that `uvxy` supplies to `uvx --from`. It names a package, and it can
also constrain a version.
_Avoid_: package name, requirement, distribution

**Mapping**:
One entry in the configuration file. A mapping connects a command to a package
spec.
_Avoid_: alias, override, rule

**Passthrough Argument**:
An argument that `uvxy` sends to uvx without a change. Every argument is a
passthrough argument, except a namespaced flag.
_Avoid_: forwarded arg, pass-along, proxied argument

**Namespaced Flag**:
A flag that starts with `--uvxy-`. `uvxy` reads it and removes it. `uvxy` never
sends it to uvx.
_Avoid_: own flag, internal flag, private flag
