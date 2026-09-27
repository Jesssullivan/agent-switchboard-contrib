# AGENTS.md

`agent-switchboard` (binary `swb`) is the tailnet-only broker for cross-harness
agent session discovery, threaded dialog and advisory task claims/handoff. It
also holds the per-host push adapter (`swb agentd`) and the hook client
(`swb hook`). Linear: TIN-4655.

**Read first:** [ADR-0001](docs/adr/0001-agent-switchboard.md). It is the
design and the rulings list (`SWB-R01`..`SWB-R23`). A design change needs a
new ruling, recorded as a dated TIN-4655 comment, and then an ADR update. An
operator question or aside is not a ruling.

**Read second:** [ADR-0002](docs/adr/0002-lgtm-plane.md), the LGTM plane
(Proposed until its PR merges). It carries the R0 rulings
`SWB-R25`..`SWB-R32`, their amendment round `SWB-R33`..`SWB-R36`
(2026-09-25), and interview completion `SWB-R37`..`SWB-R48` (2026-09-26), and the re-sequenced phase order R0 → P1a → L0 → the SWB-R27
decision → P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4, with L5 independent of
P4: it starts once the scrubbed TIN-4668 endpoint, its scrubber and the
enforced ACL all exist (SWB-R36). `SWB-R02` was reworded in the same round;
ADR-0001 keeps the superseded wording beside the new one.

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
  (SWB-R09). Tempo and Loki keep 7 d; only the broker keeps 30 d unacked
  (SWB-R25).
- Bodies go to Loki with the audit stream (SWB-R19), so a body must never
  carry a secret. They never go into Tempo span attributes; the route is
  broker stdout → Alloy with a `loki.process` stage (SWB-R26), which
  redacts with the same pattern set as the TIN-4668 collector scrubber
  (SWB-R34). The body-read ACL decision is ACL A, "A: tailnet ACL, admins +
  MCP" (SWB-R27), and bodies stay out of stdout until ACL A is enforced
  live: TIN-4670 has applied it in `Jesssullivan/tailnet-acl` and a raw
  Loki read from a non-admin tailnet node is refused (SWB-R33).
- Harness telemetry (L5): Tempo is the recency index. Harness spans carry
  tool names, file paths and ticket IDs as attributes and never bodies;
  prompts, tool input and output and responses stay in Loki (SWB-R35).
- SWB-R02, reworded 2026-09-25, verbatim: "Self-registration is
  authoritative; the broker writes through to LGTM, which is the
  read/query/context plane and never the commit path." Acks, claims,
  sequence and leases stay in the broker.
- Readers of LGTM sort by `seq`, enumerate threads only through the broker
  (a LogQL query for a thread's bodies is a sample, checked against
  `thread`'s max `seq`), and treat an absence as "expired from the view or
  not projected".

- Agentd may emit `swb` telemetry, limited to lifecycle metadata, counts
  and errors; no bodies or other content (SWB-R40, SWB-R47). This
  supersedes the September 25 broker-only writer proposal. The collector
  must prove source identity for broker/agentd lines, spans and series,
  rejecting spoofed client identity; enrichment alone is insufficient
  (SWB-R44). Consequential state remains broker-authoritative.
- P1b includes Pi registration and a threaded round trip (SWB-R37), plus
  explicit combined broker/LGTM read paths for Junie and Pi. Keep default
  groups/profiles intact and measure combined tool budgets (SWB-R41).
  Usable L0 reads are a functional prerequisite; only telemetry scrubbing
  and provenance proof follow v1 and do not block broker MVP (SWB-R48).
  Bodies stay disabled until SWB-R33/SWB-R34 qualification.
- Repeated global `msg_id` with different sender or body returns an opaque
  conflict without another message's receipt or metadata; exact retries
  remain idempotent (SWB-R46).
- Host order: Neo Claude ↔ Sting Codex acceptance, Honey/Bumble, then
  yoga/mbp-13; PZM remains behind storage delivery (SWB-R38, SWB-R45).
  Numeric agentd and metrics budgets remain proposals.
- Lab P1b contract tests use a named Honey-primary, Sting-fallback lane;
  receipts include exact source and execution host (SWB-R39).
- Honey's Grafana MCP retargets to canonical Ingress after its precise ACL
  grant and live Host-header canary; proxied Tempo tools need a separately
  measured allowlist (SWB-R42). L4's dedicated tinyland.dev owner-release
  carrier is TIN-5022, sequenced by blahaj with no date implied (SWB-R43).

## Durable notes

- **Never leave durable output** in `/tmp`, `/private/tmp` or a harness
  scratchpad. Findings, plans and receipts go to `docs/agent-notes/` (see its
  README) or a dated TIN-4655 comment.
- **Secret-bearing scratch** goes to `~/.claude/agent-notes-rescue/YYYY-MM-DD/`
  and is distilled before the task ends.
