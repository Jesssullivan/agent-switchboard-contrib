# Contributing to agent-switchboard

Thanks for helping. This page covers what a contribution needs, how changes
flow, and how to run the checks. Please also read the
[Code of Conduct](CODE_OF_CONDUCT.md).

Before you start on anything larger than a small fix, open an issue so we can
agree on the shape first. Broker behaviour is governed by the Rulings tables
in [ADR-0001](docs/adr/0001-agent-switchboard.md) and
[ADR-0002](docs/adr/0002-lgtm-plane.md); a change that contradicts a ruling
needs a new ruling before code.

## License and sign-off (DCO)

The project is licensed MIT OR Apache-2.0. There is no CLA. Instead, every
commit must carry a [Developer Certificate of Origin](https://developercertificate.org/)
sign-off, which certifies that you wrote the change or otherwise have the
right to submit it under the project's license:

```
Signed-off-by: Your Name <you@example.com>
```

`git commit -s` adds it. Use your real name and an address you can be
reached at. Maintainers check for the sign-off in review and will not merge
a commit without one (there is no automated DCO check yet).

Commits to `main` must also be signed (GPG or SSH signing); see
[GitHub's guide to signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits).

## Flow: fork first

1. Fork `xoxd-ai/agent-switchboard` and clone your fork.
2. Point the remotes the project expects: `origin` is your fork, and
   `upstream` is `xoxd-ai/agent-switchboard` with pushes disabled.
   `just fork-setup <your-github-user> <your-fork-name>` does this and
   refuses a repository whose GitHub parent is not upstream.
3. Branch from `upstream/main`. Branch names are
   `<type>/<slug>-<yyyymmdd>`, for example `fix/peers-limit-20261019`
   (maintainers add a tracker id: `<type>/tin-####-<slug>-<yyyymmdd>`).
4. Make the change, with tests, and run the checks below.
5. Push to your fork and open a pull request against `xoxd-ai/agent-switchboard`
   `main`. Fill in the template.

Keep pull requests focused: one change, with its tests and the docs it
affects (README, `docs/REFERENCE.md`, CHANGELOG `[Unreleased]`).

### CI on fork pull requests

The project's CI runs on the estate's private runners, which do not run
untrusted fork code. On a fork pull request every CI job, `ci-ok` included,
is skipped, and GitHub reports the skipped required `ci-ok` as passing: a
green check on a fork pull request means nothing ran. The full lane runs
when a maintainer queues the change in the merge queue; a maintainer may
instead land it after running the checks locally. So run the checks below
and say in the pull request what you ran.

## Running checks

```sh
nix develop            # toolchain: bazelisk, just, cargo, gh, jq, gitleaks, trufflehog
just check             # rustfmt, clippy, unit and integration tests (Bazel)
just spec-dhall        # formal-spec Dhall check (interpreters only)
just spec-quickcheck   # Haskell QuickCheck model (compiles; needs a build host)
just secrets-scan      # TruffleHog + gitleaks over the full history
```

Without Nix, `cargo test --workspace` and `cargo clippy --workspace` are a
quick first pass, but Bazel (`just check`) is the build and test authority.
Bazel needs `nix-build` on `PATH` for its pinned C toolchain on Linux.

- **Lock files:** `Cargo.lock`, `cargo-bazel-lock.json` and
  `MODULE.bazel.lock` change only together, through `just lock`, run on
  linux x86_64.
- **Release-protected inputs:** `scripts/release-check.py` hashes the build
  inputs of the current approved release (`Cargo.toml` files, `crates/`,
  `deploy/`, `tools/`, the Bazel files and lock files). Changing them is
  fine in a pull request; publishing a new image from them needs a new
  approved release record, which maintainers handle.
- **Formal spec:** edit `spec/dhall/approved-broker.dhall` together with
  `docs/releases/approved-broker.json`. See [spec/README.md](spec/README.md).

## Review and landing

A maintainer reviews every pull request. Changes land by merge commit, either
through the merge queue or by a maintainer after local validation. Release
tags and image publication are maintainer-only.

## Security issues

Do not open a public issue for a vulnerability. See [SECURITY.md](SECURITY.md).
