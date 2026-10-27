---
title: "Harness kinds hermes and kimicode, text hook output and channel --tools full"
date: 2026-10-27
status: active
summary: >-
  One shared harness list for store, broker schema, envelope schema and CLI;
  SWB_HOOK_FORMAT=text; swb channel --tools full with from and me pinned and
  claude/channel push kept Claude-only.
refs:
  - TIN-4655
---

# Harness kinds, text hook output and the full channel tool set

Authority: R-HOOK-CONVERGENCE-20261004 (R-N13 receipt) and the TIN-4655
harness lane (PR1 of the switchboard plan). PR2, sender binding, is only a
draft: [ADR-0003](../adr/0003-sender-binding.md).

## Change

- `swb_store::HARNESS_KINDS` is the one list of kinds. The store
  (`valid_agent_id` and `register`), the broker `register` enum, the agent
  id descriptions and the CLI read it. `hermes` and `kimicode` are new; `kimi` still means
  Claude Code on a Kimi model. The envelope schema pattern names the same
  kinds, and a broker test checks it against the constant.
- `swb hook` prints the UserPromptSubmit notice as plain text when
  `SWB_HOOK_FORMAT=text`. Every other value keeps the Claude Code JSON.
- `swb channel --tools full` adds `send`, `peers`, `thread`, `claim`,
  `release`, `handoff` and `claims`. No schema has `from` or `me`. A call
  that names another id gets `identity_override_denied` before any broker
  request, modelled on lab's pi agents MCP adapter. `send` refuses
  `operator_directed` and `ruling`. The `claude/channel` capability and the
  poller run in full mode only for a `claude` session.

## Evidence

- `cargo test --workspace --locked` in the flake dev shell: swb 56, broker
  28, store 43, proto 2, agentd 1 and the stamp test 1, all passing.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `just check` (Bazel `//:check`): see the pull request.

## Limits

- The pinning guards one client process. The broker still trusts a
  self-asserted sender until sender binding lands.
- An older client refuses ids with the new kinds, so a broker and its
  clients upgrade together.
- No live broker, harness or host was touched.
