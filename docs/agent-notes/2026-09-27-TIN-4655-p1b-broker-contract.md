---
title: "P1b broker envelope and lease contract correction"
date: 2026-09-27
status: active
summary: >-
  The draft broker now aligns delivered envelopes, caller lease renewal, metrics and MCP send discovery with its P1b contract.
refs:
  - TIN-4655
  - https://github.com/xoxd-ai/agent-switchboard/pull/5
---

## Mutation receipts

- SWB-R14, R-N13: declared nonnegative `delivery_count` in envelope v3 so inbox output is allowed by `additionalProperties: false`; rejected an explicitly empty optional `ruling` pointer.
- SWB-R02, R-N13: `peers` accepts an optional registered `me` and renews that lease alone. Omitted `me` is passive discovery and does not change session state. An unknown or malformed explicit caller is rejected.
- SWB-R09, R-N13: `swb_mailbox_unacked` excludes rows past `expires_at` even before an inbox poll changes stored state.
- SWB-R04, SWB-R14, R-N13: advertised MCP send schema now exposes the supported `artifacts`, `reply_expires` and `reply_format` parameters.

## Verification and boundary

Honey `just remote-check honey '~/scratch/agent-switchboard-p1b-broker-core-check'` passed all eight Bazel targets, including rustfmt, clippy, unit and integration tests. The first pass reported rustfmt differences; those were fixed before the passing run. Store regressions cover passive and explicit peer discovery, expired message metrics without inbox polling, empty rulings and repeated delivery count. Broker tests cover advertised MCP fields. No deployment, process signaling, body audit change, or post-v1 provenance/scrubbing work is in this branch.
