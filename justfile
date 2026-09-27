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
