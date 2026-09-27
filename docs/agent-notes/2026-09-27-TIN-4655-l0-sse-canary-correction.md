---
title: "L0 Host-header canary accepts 200 SSE transport"
date: 2026-09-27
status: active
summary: >-
  Correct ADR-0002's bare GET interpretation to accept 200 SSE while
  preserving initialize, Tempo tool and full L0 acceptance requirements.
refs:
  - TIN-4655
---

R-N13 receipt, SWB-R29 documentation correction. On 2026-09-27 UTC
(2026-09-26 EDT), the L0 read-only packet exposed that ADR-0002 named only
405/406 for bare GET transport acceptance. Lab's committed canary accepts
200 with `Content-Type: text/event-stream` too; other 200 content types
fail, and a Host allowlist rejection remains 403.

Sources read:

- `lab@430f6e37ed9657478f61d2708ff56bb7b3d35340`,
  `scripts/validation/mcp-plane-canary.py:5-14,97-105,249-274,315-333`;
  file clean at readback, last changed at
  `3389f40abf429ab902c0e5166f4530d073ea2a68`.
- TIN-4655 dated comment `750738aa-3fda-4828-9857-a934c2a20920`, the
  read-only L0 canary packet. It records the expected status interpretation
  and requires candidate initialize/version, Tempo tools and measured
  budgets before acceptance. No live L0 acceptance is claimed.

Changed only ADR-0002's Host-header interpretation and corresponding exit,
plus this receipt (SWB-R29; R-N13). Bare GET success remains transport
proof only. JSON-RPC initialize must identify the candidate version;
`tools/list` must contain `search_tempo_traces` and `get_tempo_trace`.
Existing successful Tempo queries, tool budgets, version/argv readback and
operator canary lifecycle receipts remain full L0 requirements. No new
ruling, L0 service action, build, process signal or Linear edit occurred.

Checks: `git diff --check`, local Markdown link targets and manual review
of the narrow diff. Signed commit and exact-branch fork push update PR #4
under SWB-R08/SWB-R13/SWB-R30; no merge or enqueue (R-N13).
