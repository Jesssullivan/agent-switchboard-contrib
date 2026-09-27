---
title: "P1b retention and graceful shutdown fixes"
date: 2026-09-27
status: active
summary: >-
  Records isolated source fixes for idle message retention and broker listener drain, based on the signed local integration tree.
refs:
  - TIN-4655
  - SWB-R09
  - SWB-R49
  - R-N11
  - R-N13
---

## Source and scope

- **SWB-R49 / R-N13 receipt:** Created the isolated worktree `agent-switchboard-retention-drain` on branch `fix/tin-4655-retention-drain-20260927` from signed local integration commit `6330f2e3dfdb46ac92e0e1df66c850ce43c46639`. Existing #5/#6 worktrees and the published candidate were not changed.
- **SWB-R09 / R-N13 receipt:** Pruning runs when the store opens and hourly while the broker is idle. The existing 7-day acked and 30-day unacked cutoffs and thread high-water sequence remain unchanged.
- **R-N11 / R-N13 receipt:** The broker listens for its own SIGTERM or Ctrl-C and asks both Axum listeners to drain active requests. The blahaj stack grants the pod 60 seconds; the broker allows 30 seconds for ordinary requests, then cancels rmcp streams and allows five more seconds for listener closure. The final timeout bounds the listener futures, not arbitrary blocking SQLite tasks or unconditional process exit. This source change sends no signal and performs no process control.
- **SWB-R49 / R-N13 receipt:** Tokio signal support and the broker's SQLite test dependency were added through the lock workflow on Honey. The final `just lock` status command failed because the rsynced scratch tree deliberately has no `.git`; Bazel lock generation steps completed, and the generated files were copied back for verification.

## Validation

Honey `just check`: passed all eight Bazel targets, including rustfmt, Clippy, the store tests, and the broker tests. The idle timer test ages the row only after broker readiness, so startup pruning cannot make it pass. The shutdown tests prove an inbox handler entered before shutdown and that an initialized MCP GET SSE stream closes after the bounded drain. No image publication, push, or PR mutation in this pass.
