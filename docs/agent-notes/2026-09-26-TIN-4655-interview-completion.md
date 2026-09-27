---
title: "Agent-switchboard interview completion and v1 boundary"
date: 2026-09-26
status: active
summary: >-
  Carry SWB-R37 through SWB-R48 into both ADRs and agent instructions,
  retaining L0 and combined Junie/Pi access in P1b while deferring telemetry proof.
refs:
  - TIN-4655
  - TIN-5022
---

R-N13 receipt for this documentation-only session. Authority: dated
TIN-4655 comments `2aeb4bee` (SWB-R37–39), `8a524db4` (SWB-R40–42),
`f1893e4c` (SWB-R43–47), and `ef8a1cd2` (SWB-R48), read on 2026-09-26.
TIN-5022 was read back as the dedicated L4 tinyland.dev owner-release
carrier, sequenced by blahaj with no date implied (SWB-R43).

## Changes and authority

- ADR-0001: Pi and combined Junie/Pi broker/LGTM P1b acceptance, L0
  dependency, host order and remote test lane (SWB-R37–39, SWB-R41,
  SWB-R45, SWB-R48); metadata-only agentd telemetry (SWB-R40/SWB-R47);
  opaque duplicate conflicts (SWB-R46).
- ADR-0002: all twelve ruling rows with exact answer quotes and carriers;
  current clauses updated and answered interview questions replaced with
  a dated resolution map (SWB-R37–48). Numeric budgets remain proposed.
  Historical refutation and superseded wording remain marked as history.
- AGENTS.md and README orientation aligned with those rulings. The notes
  README extends its ruling range. No INDEX regeneration.
- SWB-R48 defers only telemetry scrubbing/provenance qualification. L0's
  usable read plane and combined tool budget proof remain functional P1b
  requirements. SWB-R33/SWB-R34 keep message bodies disabled until qualified.

## Verification and delivery

Documentation-only checks: `git diff --check`, local Markdown-link target
validation, and a check that SWB-R37–SWB-R48 each has one ruling row.
No compiler, build or test workload ran on Neo. No process was signaled
(R-N11). No Linear description was changed (R-N13).

The signed amendment commit is pushed only to the existing fork branch
`docs/tin-4655-adr-0002-lgtm-plane-20260925` for upstream PR #4
(SWB-R08, SWB-R13, SWB-R30; R-N13). This receipt does not claim an
upstream merge or any functional delivery. PR #4 remains unqueued until
#2 and #3 land under the standing sequence.
