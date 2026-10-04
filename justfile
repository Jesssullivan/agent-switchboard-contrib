set shell := ["bash", "-euo", "pipefail", "-c"]

# Mirror the CI driver: never let an ambient crate_universe repin or generator
# override leak into a normal build.
clean_bazel_env := "env -u CARGO_BAZEL_DEBUG -u CARGO_BAZEL_GENERATOR_SHA256 -u CARGO_BAZEL_GENERATOR_URL -u CARGO_BAZEL_ISOLATED -u CARGO_BAZEL_REPIN -u CARGO_BAZEL_REPIN_ONLY -u CARGO_BAZEL_TIMEOUT -u REPIN"

upstream_repo := "xoxd-ai/agent-switchboard"

default:
    @just --list

# Bazel is the build and test authority. Never run on neo (teletype seat;
# the local-build guard refuses it): use `just remote-check` from neo.
# Run rustfmt, clippy, unit and integration tests (the labels CI runs).
check:
    {{clean_bazel_env}} bazelisk test --lockfile_mode=error //:check

# Build the application and package targets CI builds.
build:
    {{clean_bazel_env}} bazelisk build --lockfile_mode=error //:build //deploy:image //deploy:image.digest

# Commit all three. Run on linux x86_64 (sting or honey), the platform CI
# checks the locks on.
# Regenerate Cargo.lock, cargo-bazel-lock.json and MODULE.bazel.lock together.
lock:
    {{clean_bazel_env}} CARGO_BAZEL_REPIN=1 bazelisk mod deps --lockfile_mode=update >/dev/null
    {{clean_bazel_env}} CARGO_BAZEL_REPIN=1 bazelisk build --lockfile_mode=update @crates//:defs.bzl
    {{clean_bazel_env}} bazelisk build --lockfile_mode=update --nobuild //...
    {{clean_bazel_env}} bazelisk mod deps --lockfile_mode=update >/dev/null
    git status --short -- MODULE.bazel.lock Cargo.lock cargo-bazel-lock.json

# The tree is rsynced to a scratch dir on the build host; neo builds nothing.
# Run `just check` on a build host from a teletype seat (neo).
remote-check host="sting" dir="~/scratch/agent-switchboard-check":
    ssh {{host}} 'mkdir -p {{dir}}'
    rsync -a --delete --exclude '/bazel-*' --exclude '/target/' --exclude '/.git/' ./ {{host}}:{{dir}}/
    ssh {{host}} 'cd {{dir}} && just check'

# SWB-R49 local integration while GF is in development (TIN-4655).
# Pin each PR to its reviewed full SHA. This creates a separate,
# signed local merge tree; it never pushes or changes GitHub PR/main state.
# Example: just local-integrate '2@<full-sha> 3@<full-sha> 4@<full-sha> 5@<full-sha>'
local-integrate refs:
    bash ./scripts/local-integrate.sh {{ quote(refs) }}

local-integrate-dry-run refs:
    bash ./scripts/local-integrate.sh --dry-run {{ quote(refs) }}

# Keep the existing integration candidate and create a second, immutable one.
# Example: just local-integrate-named second '2@<full-sha> 3@<full-sha>'
local-integrate-named name refs:
    bash ./scripts/local-integrate.sh --name {{ quote(name) }} {{ quote(refs) }}

local-integrate-named-dry-run name refs:
    bash ./scripts/local-integrate.sh --dry-run --name {{ quote(name) }} {{ quote(refs) }}

# SWB-R53 / R-N13: read-only approved-source and supplied OCI evidence checks.
# No build, publication, registry credentials or deployment admission.
[positional-arguments]
release-check *args:
    python3 ./scripts/release-check.py "$@"

release-check-test:
    python3 -m unittest discover -s scripts -p test_release_check.py

# TIN-4655 (SWB-R53): verify a signed annotated release tag at HEAD that is on
# upstream main, plus the approved source inputs. Read-only; the release
# workflow runs the same gate before it builds or pushes anything.
release-check-tag tag main_ref="upstream/main":
    python3 ./scripts/release-check.py --tag {{ quote(tag) }} --main-ref {{ quote(main_ref) }}

# The scanners CI's `secrets-scan` job runs (TruffleHog --only-verified, then
# gitleaks with .gitleaks.toml) over the full history. Both ship in the
# devShell: `nix develop --command just secrets-scan`. This is the pre-queue
# signal for a fork PR, whose CI scan is skipped until merge_group.
secrets-scan:
    #!/usr/bin/env bash
    set -euo pipefail
    for tool in trufflehog gitleaks; do
      command -v "$tool" >/dev/null || { echo "secrets-scan: $tool is not on PATH (use nix develop)" >&2; exit 1; }
    done
    trufflehog --no-update git "file://$PWD" --only-verified --fail
    gitleaks git --config .gitleaks.toml --redact --exit-code 1 .

# PRs go from the fork to upstream main through the merge queue (ADR-0001).
# Set remotes: origin = your private fork (the only push target), upstream = xoxd-ai with push DISABLED.
fork-setup fork_owner="Jesssullivan":
    #!/usr/bin/env bash
    set -euo pipefail
    upstream_url="https://github.com/{{upstream_repo}}.git"
    fork_url="https://github.com/{{fork_owner}}/agent-switchboard.git"
    if git remote get-url upstream >/dev/null 2>&1; then
      git remote set-url upstream "$upstream_url"
    else
      git remote add upstream "$upstream_url"
    fi
    git remote set-url --push upstream DISABLED
    if git remote get-url origin >/dev/null 2>&1; then
      git remote set-url origin "$fork_url"
    else
      git remote add origin "$fork_url"
    fi
    git fetch upstream
    git fetch origin
    if git show-ref --verify --quiet refs/heads/main; then
      git branch --set-upstream-to=upstream/main main
    fi
    git remote -v

# Start a branch named per the convention: <type>/tin-####-<slug>-<yyyymmdd>.
branch type tin slug:
    git fetch upstream
    git switch -c "{{type}}/tin-{{tin}}-{{slug}}-$(date -u +%Y%m%d)" upstream/main

# Build the immutable Linux/amd64 OCI image and print the exact manifest digest.
image:
    {{clean_bazel_env}} bazelisk build --lockfile_mode=error //deploy:image //deploy:image.digest
    @digest="$(cat bazel-bin/deploy/image.json.sha256)" && printf 'ghcr.io/xoxd-ai/agent-switchboard@%s\n' "$digest"

# Stub (P1b): end-to-end round trip against a live broker.
e2e:
    @echo "e2e: lands with the P1b broker MVP (ADR-0001, phase P1b exit)" >&2
    @exit 1
