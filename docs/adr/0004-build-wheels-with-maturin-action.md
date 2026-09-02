# Build wheels with maturin-action, not cibuildwheel

`uvxy` builds its wheels with `PyO3/maturin-action`. It publishes seven wheels
and one sdist:

- macOS: arm64, x86_64
- manylinux: x86_64, aarch64
- musllinux: x86_64, aarch64
- Windows: x86_64

## Why not cibuildwheel

`uvxy` does not link PyO3. maturin therefore tags its wheel `py3-none-<platform>`,
and one wheel serves every Python version. uv ships the same way. Its wheel
records `Tag: py3-none-macosx_10_12_x86_64`.

cibuildwheel builds one wheel for each CPython version. Against a
version-independent tag, it builds the same wheel seven times for each platform,
and the results collide by filename. You can suppress this with
`build = "cp312-*"`, but you then carry a matrix that you also disable.

cibuildwheel also repairs wheels with `auditwheel` and tests them against many
interpreters. A static Rust binary never links `libpython`, so those steps find
nothing.

The cibuildwheel FAQ agrees. It states that Rust wheels avoid the GLIBC problems
that manylinux solves, and it directs Rust projects to maturin-action.

## Consequences

- The workflow does not match other Python projects that use cibuildwheel.
- musllinux comes from maturin-action containers, not from cibuildwheel ones.
