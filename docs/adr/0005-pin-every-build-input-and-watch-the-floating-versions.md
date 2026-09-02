# Pin every build input, and watch the floating versions in a scheduled job

Every input that produces a build is pinned:

| Input          | Pinned by                                     |
|----------------|-----------------------------------------------|
| Rust toolchain | `rust-toolchain.toml`, channel `1.89.0`       |
| Dependencies   | `Cargo.lock`, and `--locked` on every command |
| maturin        | `maturin-version` on every maturin-action job |
| manylinux image| `manylinux: "2014"`, not `auto`               |

The `Drift` workflow reads the floating versions instead. It runs every day. One
job reads the newest uv. One job reads the newest stable Rust.

## Why

A pull request must fail for one reason, and that reason must be the pull
request. An unpinned input breaks that rule. Rust adds clippy lints about every
six weeks, and `cargo clippy -- -D warnings` turns those lints into a failed
build. A contributor then reads a red run that their own change did not cause.

The information still has value. A new lint is worth reading, and a new uv
release is worth testing. So the checks still run against the newest versions.
They run on a schedule, on the default branch, where a failure interrupts
nobody.

## Why `--locked`

`Cargo.lock` sits in the repository, and cargo reads it. Without `--locked`,
cargo also rewrites it when it wants to, and the build then uses a dependency
that no commit records. `--locked` turns that rewrite into an error.

## Consequences

- Nothing tests uvxy against the newest stable Rust before the next scheduled
  run. A pull request that a new lint rejects still merges.
- Someone must raise the channel in `rust-toolchain.toml`. The scheduled job
  reports when, and it names the version.
- `maturin-version` repeats in five jobs. A bump touches five lines.
