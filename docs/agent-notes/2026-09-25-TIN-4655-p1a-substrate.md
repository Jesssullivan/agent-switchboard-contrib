---
title: "P1a substrate: repo, scaffold, ruleset, private fork and exit-test receipts"
date: 2026-09-25
status: active
summary: >-
  xoxd-ai/agent-switchboard created; scaffold landed via PR #1 on an API root
  commit; ruleset 23997034 active; private fork wired. The queue outcome for
  this exit-test PR is recorded on TIN-4655.
refs:
  - TIN-4655
  - "#1"
---

# P1a substrate receipts (2026-09-25)

The lane ran from neo. Nothing was compiled on neo: the locks and the compile
check ran on sting in `~/scratch/agent-switchboard-p1a`.

## Receipts

| Step | Result | Ruling |
| --- | --- | --- |
| Create `xoxd-ai/agent-switchboard` | Private, created empty | SWB-R08, SWB-R21 |
| Scaffold, first attempt | Signed `c4ba32c`. The direct push to `main` was refused by the Home Manager pre-push hook (R-N12 stop) and never pushed | SWB-R21 |
| Root commit on `main` | `89bfa39` via the contents API, the only direct write to `main`. GitHub reports it `unsigned`: contents-API commits made with this token are not web-flow-signed | SWB-R22 |
| Scaffold PR | #1. Signed `2cff51d` (same tree as `c4ba32c`), merged as `2a11acb` (verified) with `--merge` while `main` was unprotected | SWB-R22 |
| Ruleset | `23997034` `main-merge-queue`, active, target `~DEFAULT_BRANCH`, no bypass actors (details below) | SWB-R22 |
| Org fork setting | Operator turned it on in the UI; read back `members_can_fork_private_repositories=true` and repo `allow_forking=true` | SWB-R23 |
| Fork | `Jesssullivan/agent-switchboard`, private, parent xoxd-ai | SWB-R08, SWB-R13 |
| Remotes | `just fork-setup`: `origin` is the fork; `upstream` is xoxd-ai with push URL `DISABLED` | SWB-R08 |
| Exit test | This PR, from the fork through the merge queue | SWB-R13 |

The ruleset rules:

- deletion and non-fast-forward blocked;
- required signatures;
- a pull request with 0 approvals and merge method `merge`;
- required check `ci-ok`, from integration 15368 (GitHub Actions), not
  strict;
- `merge_queue`: MERGE, ALLGREEN, build 1, merge up to 3, minimum 1,
  2-minute wait, 180-minute check timeout.

GitHub added `require_extra_approval_for_unattributed_changes: true` by
default, as on lab's ruleset.

## Pre-push refusal (R-N12), verbatim

The last three lines were captured live. The first lines are the same echo
branch of `~/.config/git/hooks/pre-push` (Check 2), with `pushed_branch=main`.

```text
BLOCKED: Direct push to protected branch 'main' is not allowed

Please create a feature branch and submit a merge/pull request:
  git checkout -b feature/my-change
  git push -u origin feature/my-change

This check cannot be bypassed with environment variables.
error: failed to push some refs to 'https://github.com/xoxd-ai/agent-switchboard.git'
```

Check 2 skips only deletions, so it also blocks creating `main` in an empty
repo. The operator's answer was SWB-R22.

## CI emulation on sting (before any push)

The emulation mirrors the ci-templates v5.1.1 `bazelisk-ci` driver: fresh
`HOME` and `--output_user_root`, `--ignore_all_rc_files`, and
`--platforms=//platforms:x86_64-unknown-linux-gnu`.

- The lock-drift check `mod deps --lockfile_mode=update` left a committed
  tree clean.
- Every group passed with `--lockfile_mode=error`:
  - `//:fmt`, `//:clippy`, `//:build`;
  - `//:unit_tests` (5 of 5) and `//:integration_tests` (1 of 1);
  - `//deploy:swb_layer`.
- The locks did not change during the suite.
- `swb version` works, and `swb hook claude` exits 0.
- Host note: sting's linker warns "the gold linker is deprecated and has known
  bugs with Rust". It is a warning only.
- `tinyland.repo.json` validates against the site.scaffold manifest schema at
  6c58bb6, and the envelope schema passes the 2020-12 metaschema check.

## Facts for the next pass

- **ci-templates pin:** `ae836d8400d5784d74af4fecc020f225d1c2d08e` is v5.1.1.
  The rust-lane files are identical at `origin/main` `c732248`.
- **This repo is the template's first live consumer.** prompt-pulse's
  `estate-ci.yml` is inert behind `ESTATE_CI_ENABLED`.
  - The PR #1 and push-to-main runs queued `rust / trust-gate` on the bare
    `tinyland-nix` label, at 12:38Z.
  - What the lane does on `merge_group` is recorded on TIN-4655: runner group
    access for `tinyland-infra`, the `[tinyland-nix, Linux, X64]` label
    match, and `TINYLAND_CI_BAZELISK_BIN` custody.
- **Org CodeQL default setup.** The org runs it on new repos: a dynamic
  "CodeQL Setup" run on GitHub-hosted `ubuntu-latest` started for this repo.
  It comes from org security configuration, not this repo's workflows.
- **Fork PR workflow policy** is inherited:
  - run `true`, write tokens `true`, secrets `true`, approval `false`;
  - the ci.yml `if:` guards skip both jobs on fork PRs, and the workflow
    token is `contents: read`.
