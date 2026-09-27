# AGENTS.md

`agent-switchboard` (binary `swb`) is the tailnet-only broker for cross-harness
agent session discovery, threaded dialog and advisory task claims/handoff. It
also holds the per-host push adapter (`swb agentd`) and the hook client
(`swb hook`). Linear: TIN-4655.

**Read first:** [ADR-0001](docs/adr/0001-agent-switchboard.md). It is the
design and the rulings list (`SWB-R01`..`SWB-R23`). A design change needs a
new ruling, recorded as a dated TIN-4655 comment, and then an ADR update. An
operator question or aside is not a ruling.

## Estate rulings that bind here

The fuller text lives in `xoxd-ai/lab` `AGENTS.md` (TIN-3692):

- **R-N11: process control is absolute.** Agents never signal any process, on
  any host, in any form. That covers `kill`/`pkill`/`killall`, the tmux kill
  subcommands, `systemctl stop|kill` and `launchctl kill|bootout`, including
  their literal-PID forms. The product is bound the same way: no broker tool,
  hook or agentd path acts on a process, a tmux pane or another session's
  claim. agentd never unlinks a socket.
- **R-N12: a guard-hook refusal is a stop.** Quote it verbatim, propose at
  most one materially different alternative, and ask before running it.
- **R-N13: ratification.**
  - Every mutating step cites a ruling ID in its receipt.
  - Every session writes a `docs/agent-notes/` entry before ending.
  - Ticket descriptions are superseded by dated comments, never rewritten.

## Source of truth

When sources disagree, prefer them in this order:

1. `MODULE.bazel`, `.bazelversion`, `BUILD.bazel` files and the three lock
   files;
2. `justfile`;
3. `.github/workflows/ci.yml`;
4. `docs/adr/`;
5. `README.md`, which is orientation only.

## Build placement

- **Bazel is the build, test and package authority.** Cargo is a diagnostic
  mirror only.
- **Never compile on neo.** neo is the teletype seat, and its local-build
  guard refuses `bazel`/`cargo` builds. Run `just remote-check` from neo, or
  run `just check` on sting or honey. `cargo metadata` and
  `cargo generate-lockfile` are fine anywhere.
- **Change the three lock files only together, with `just lock`.** They are
  `Cargo.lock`, `cargo-bazel-lock.json` and `MODULE.bazel.lock`. Run it on
  linux x86_64 (sting or honey), which is the platform CI checks them on.
- **CI runs Bazel with `--ignore_all_rc_files`.** Nothing a CI target needs
  may live in `.bazelrc`.

## Fork convention and CI

- **Remotes:** `origin` is your private fork and the only push target.
  `upstream` is `xoxd-ai/agent-switchboard`, with its push URL set to
  `DISABLED`. `just fork-setup` configures both.
- **Branches:** `<type>/tin-####-<slug>-<yyyymmdd>`. `just branch <type>
  <tin> <slug>` creates one.
- **PRs** go from the fork to `upstream/main` and land only through the merge
  queue.
  - The ruleset on `main` requires signed commits, a PR (0 approvals, merge
    method `merge`) and the `ci-ok` check.
  - The queue settings are MERGE, one entry built at a time, ALLGREEN.
  - Force-push and deletion are blocked, and nothing bypasses the ruleset.
- **Fork PRs are gated in the merge queue.** The ci-templates Rust lane
  refuses private runners for fork PRs, and hosted runners are forbidden. So
  on a fork PR both jobs are skipped and `ci-ok` reports as passing. The full
  lane runs on `merge_group`. Run `just check` (or `just remote-check`)
  before queueing.
- **Commits** are GPG-signed, with no AI attribution lines.

## Product invariants

These are ruled; see the ADR:

- The broker stamps `authority: peer` on every message and never emits
  `operator`. `operator_directed` is the sender's claim and needs a `ruling`
  pointer, which receivers check before acting (SWB-R14).
- Claims are advisory and always succeed. The only refusal is a second
  `exclusive` claim, which returns `held_by` (SWB-R16).
- Hooks time out after 2 s and always exit 0 (SWB-R10).
- Linear access is read plus handoff-receipt comments only. The broker never
  moves state or edits descriptions (SWB-R15).
- Retention is 7 d acked and 30 d unacked; the TTL is 72 h, at most 14 d
  (SWB-R09).
- Bodies go to Loki with the audit stream (SWB-R19), so a body must never
  carry a secret.

## Durable notes

- **Never leave durable output** in `/tmp`, `/private/tmp` or a harness
  scratchpad. Findings, plans and receipts go to `docs/agent-notes/` (see its
  README) or a dated TIN-4655 comment.
- **Secret-bearing scratch** goes to `~/.claude/agent-notes-rescue/YYYY-MM-DD/`
  and is distilled before the task ends.
