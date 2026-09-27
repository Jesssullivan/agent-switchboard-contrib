---
title: "P1b rmcp Streamable HTTP transport receipt"
date: 2026-09-26
status: active
summary: >-
  Draft PR #5 now uses rmcp Streamable HTTP, with an SDK session round trip verified by Bazel on honey.
refs:
  - TIN-4655
---

## Mutation receipt

- SWB-R04 and SWB-R14: replaced the hand-written `/mcp` JSON-RPC handler with
  rmcp 3.4.1 `StreamableHttpService` mounted at `/mcp` in Axum. The five P1b
  tools call the same SQLite-backed operations as REST. `swb serve` now runs
  both Axum listeners on Tokio; no product path signals a process (R-N11).
- SWB-R14: tool definitions advertise object schemas and the server uses the
  SDK's initialize/session, `tools/list` and `tools/call` handling. A real
  Streamable HTTP session test exercises register, peers, send, inbox and ack,
  including `authority: peer` stamping. The opaque REST conflict test remains.
- SWB-R33 and SWB-R34: broker audit bodies remain absent from stdout.
- R-N13: regenerated Cargo, crate-universe and Bazel module locks together
  on honey. The remote scratch copy excludes `.git`, so the final `git status`
  line of `just lock` exits 128 after all regeneration steps complete; the
  three generated files were copied into the worktree as one set.

## Verification and delivery boundary

Honey `just remote-check` passed all eight Bazel check targets after the SDK
round-trip test; Honey `just build` passed `//:build` and
`//deploy:swb_layer`. Neo compiled nothing. No deployment or live tailnet
probe occurred.

rmcp validates the HTTP `Host` header. `SWB_MCP_ALLOWED_HOSTS` is a
comma-separated production setting for the exact tailnet hostname(s); the
source defaults to loopback hosts for local tests. The blahaj deployment
owner must declare the live values before activation.
