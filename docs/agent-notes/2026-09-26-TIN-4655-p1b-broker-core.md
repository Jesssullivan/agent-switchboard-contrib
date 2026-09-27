---
title: "P1b broker core implementation receipt"
date: 2026-09-26
status: active
summary: >-
  Broker core development on an isolated branch from upstream main; deployment and live acceptance remain separate.
refs:
  - TIN-4655
---

## Source and boundary

Worktree: `/Users/jess/git/agent-switchboard-p1b-broker-core`, branch
`feat/tin-4655-p1b-broker-core-20260926`, based on upstream/main `2a11acb`.
P1a #2, ADR #3-#4 and L0 have not landed on this base. No deployment,
process signaling, or sibling-repo mutation occurred (R-N11).

## Mutation receipts

- SWB-R02, SWB-R09, SWB-R14: added SQLite WAL/FULL coordination state,
  register/peers/send/inbox/ack, per-thread transactional sequence, TTL,
  7-day acked and 30-day unacked pruning, and peer-authority envelope stamping.
- SWB-R46: an exact global `msg_id` retry returns the stored envelope without
  another insert; a different sender or body yields only an opaque conflict.
- SWB-R19, SWB-R33, SWB-R34: broker audit stdout is metadata only, with a
  keyed body HMAC held in the broker SQLite store. Body text stays absent
  pending live ACL and redaction proof.
- SWB-R04, SWB-R14: added HTTP `/mcp`, REST `/v1/*` and `:9090/metrics` for
  the P1b tool set. Binding and deployment remain the blahaj owner lane.
- R-N13: regenerated Cargo, crate-universe, and Bazel module locks together
  on honey. `just lock` completed all regeneration commands in the remote
  scratch copy; its final `git status` line cannot succeed there because the
  scratch copy deliberately excludes `.git`.

## Verification

Honey `just remote-check` passed all eight Bazel check targets (rustfmt,
clippy, unit and integration tests) after the final retention edit. Honey
`just build` passed for `//:build` and `//deploy:swb_layer`. A broker test
opens a one-request TCP listener and completes an MCP initialize exchange;
store tests cover idempotency, opaque conflict, ack and WAL persistence
across reopening. The remote checks execute on honey; neo runs only editing
and orchestration.

## Follow-on boundaries

This branch is broker core only. P1b's lab projection, Codex binding and
attended live round trip are separate lanes. The L2 transactional outbox and
Tempo projector follow ADR-0002's phase order. No body stdout change is
permitted without the SWB-R33/34 evidence.
