# Releasing uvxy

`uvxy` publishes to PyPI from GitHub Actions. The workflow holds no API token.
It uses trusted publishing, and PyPI reads an OIDC token from the run.

## Set this up once

1. Create the `pypi` environment. Open the repository settings, open
   **Environments**, and add an environment named `pypi`. The publish job names
   that environment, and the job fails while the environment is absent.

2. Add a pending publisher on PyPI. The project does not exist yet, so PyPI
   calls this a *pending* publisher. Open **Account settings**, then
   **Publishing**. Enter these five values. Each one must match the workflow.

   | Field             | Value        |
   |-------------------|--------------|
   | PyPI project name | `uvxy`       |
   | Owner             | `chadrik`    |
   | Repository name   | `uvxy`       |
   | Workflow name     | `release.yml`|
   | Environment name  | `pypi`       |

## Release

1. Set the version in `Cargo.toml`. That file is the only source of the
   version. `pyproject.toml` declares `dynamic = ["version"]`, and maturin
   reads `Cargo.toml`.

2. Commit that change, and merge it into `main`.

3. Tag the commit, and push the tag.

   ```console
   $ git tag -a v0.1.0 -m "uvxy 0.1.0"
   $ git push origin v0.1.0
   ```

4. Read the run. The `check the tag against Cargo.toml` job runs first. Eight
   build jobs follow it. The publish job runs last.

5. Install the release, and run it.

   ```console
   $ uv tool install uvxy
   $ uvxy --uvxy-version
   ```

## Rehearse a release

Start the Release workflow by hand, from the Actions tab. A manual run builds
all seven wheels and the sdist, and it uploads them as artifacts. It publishes
nothing, because the publish job runs only for a tag.

## The tag and the version must agree

maturin reads the version from `Cargo.toml`, and it ignores the tag. A tag of
`v0.2.0` against a `Cargo.toml` that names `0.1.0` therefore builds `0.1.0`.

A PyPI version is permanent. You cannot upload a version twice, and a yank does
not release the number. So the `check the tag against Cargo.toml` job compares
the two and stops the release when they disagree. Every build job waits for
that job.

## After a failed publish

The build jobs write no permanent record, so you may repeat them. Delete the
tag, correct the problem, and tag again.

```console
$ git tag -d v0.1.0
$ git push origin :refs/tags/v0.1.0
```

A failure *after* PyPI accepts a file is different. That version is then
permanent. Raise the patch version, and release again.
