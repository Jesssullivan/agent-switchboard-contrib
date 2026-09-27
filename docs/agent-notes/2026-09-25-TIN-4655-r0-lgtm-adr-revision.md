---
title: "R0 LGTM rulings applied: ADR-0001 revised and ADR-0002 (the LGTM plane) added"
date: 2026-09-25
status: active
summary: >-
  The R0 rulings (TIN-4655 comment 73f1ce72) are carried: SWB-R02 reworded,
  SWB-R25 to SWB-R32 recorded in ADR-0002 (Proposed), the listed critique
  fixes and the Opus refutation's fixes applied, citations re-pinned.
refs:
  - TIN-4655
  - TIN-4668
  - TIN-4670
---

# R0 receipt: ADR-0001 revision and ADR-0002 (2026-09-25)

Fable synthesis seat, per SWB-R30 ("A Fable lane revises ADR-0001 and adds
ADR-0002 (the LGTM plane), folding in every critique fix. Opus refutes it,
then a PR goes through the fork."). This note is the R0 durable carrier in
this repo, beside the dated TIN-4655 comment (R-N13). ADR-0002 is
**Proposed** until its PR merges; nothing in it is a ruling of this seat's
own making.

## What changed

| File | Change | Rulings |
| --- | --- | --- |
| `docs/adr/0001-agent-switchboard.md` | SWB-R02 reworded with the superseded wording kept; SWB-R25 note on retention; SWB-R26/R27 additions to the audit block (keyed `body_hmac`, the scrubber gap named as open); the agentd-never-emits-OTLP constraint carried as design, not ruled; Codex identity via `swb whoami` with the binding marked unproven and its own P1b sub-exit; `msg_id` global uniqueness stated; enrollment path and the Junie/Pi lookup gap; the LGTM section retitled and the dashboards/alerts split (SWB-R28); a "Deltas from the R0 LGTM round" table; phases re-sequenced R0 → P1a → L0 → SWB-R27 → P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4 → L5 with operator-performed drills; P4 back to "Codex push"; numeric neo agentd bound proposed in P2; rulings table rows R02, R03, R09, R17, R19 annotated | SWB-R02, R25–R32 |
| `docs/adr/0002-lgtm-plane.md` | New: context with pinned estate facts (including the retained tailnet OTLP path and TIN-4670), principles, ownership table, data model (one trace per event, broker-derived ids, keyed body hash, Loki line schema and `loki.process` stage, Mimir series), lookup recipes (TraceQL, LogQL sample-only, PromQL; thread enumeration broker-only), outbox plus set-based reconciliation, enrollment corrections, L5 telemetry with the scrubbed endpoint named and the old one forbidden, phases L0–L5 with observable exits and the L0 canary gate, risks, open rulings, the R25–R32 table with quote discipline, and the refutation review | SWB-R25–R32 |
| `README.md` | ADR-0002 link; phase order with the SWB-R27 step | — |
| `AGENTS.md` | "Read second" paragraph for ADR-0002; product invariants for R02 (verbatim), R25, R26, R27 and the reader rules; the sole-writer constraint listed as proposed, not ruled | — |
| `docs/agent-notes/README.md` | Distilled rulings go to the ADR that owns the area (ADR-0001 or ADR-0002) | — |

## What this note does not claim

It does not claim that every possible critique fix is folded in. It claims
that the nine fixes comment `73f1ce72` lists under "Critique fixes the ADR
revision must carry" are carried (ADR-0002 → Rulings, the landing table),
and that the Opus refutation's 23 problems and 7 ruling-fidelity findings
are each handled as ADR-0002 → Refutation review records. Numeric bounds,
the outbox plus gauge, the reconciliation cadence and the keyed hash are
proposals for the operator to confirm, listed under ADR-0002 → Open
rulings.

## Refutation (SWB-R30, 2026-09-25)

Opus verdict: "ship-with-fixes". None of its problems was rejected. The
ones that needed an operator decision became Open rulings items rather than
decisions: SWB-R27's carrier and its enforcement gate (the operator's
"A: tailnet ACL, admins + MCP" answer lives on TIN-4668 `1001c0fb`), the
scope of ruling 2's Tempo ban, whether L5 waits for P4, the `swb`-bodies
scrubber gap, the agentd sole-writer constraint, and the Pi profile's phase.

## Evidence pins

- `xoxd-ai/tinyland.dev` main `68a16f652223d85c3b93ac2cdc01167749e3a3b6`
  (2026-09-25T00:52Z, still the tip at 19:00Z): every tinyland.dev line
  re-read there through `gh api` on 2026-09-25. The plan's `94535f35`
  citations were stale.
- `xoxd-ai/blahaj` main `2071613ce8097afda3737c1a7d79a48a8a557f53` (main
  moved to `917e952b` at 18:18Z; the cited retained-services file has the
  same blob at both); `xoxd-ai/lab` main
  `3d755193b0d8657213ef41c0aaad8ed319857a4e`; `Jesssullivan/tailnet-acl`
  main `84ba982964283986222af07a3d1cd2675f28bce9`.
- Upstream docs: Tempo `main` `da9051bd`, OTel specification `main`
  `cac5b81e`, Prometheus `main` `270db291`, Loki `main` `98555bd7`, lines
  re-read 2026-09-25; Tempo `v2.7.2` `configuration/_index.md:240`
  (`max_span_attr_byte` 2048); `v3.0.3` `operations/dedicated_columns.md:68-69`.
- mcp-grafana `v1.6.0` (2026-09-25): `CHANGELOG.md:136` (1.2.0 loopback
  Host check) and `:85` (1.4.0 "`--allowed-hosts` is now honored … via a
  loopback reverse proxy") both recorded; the L0 canary decides.
- Operator statements after R0, quoted where cited: TIN-4655 `26a311db`;
  TIN-4668 `875fc21f`, `1001c0fb`, `a9701697`, `605d0cc7`; TIN-4670's
  description.

## Not on this branch

- The P1a note `docs/agent-notes/2026-09-25-TIN-4655-p1a-substrate.md`
  rides in PR #2 and is not on `upstream/main`; it cites SWB-R08/R13/R21/
  R22/R23 only and neither the old R02 wording nor the phase list, so it
  needs no change.
- PR #2 adds SWB-R22/R23 rows after SWB-R21 and updates `AGENTS.md`'s
  ruling range; PR #3 appends a History section (SWB-R24). This branch
  leaves those regions untouched and numbers from SWB-R25.

## Next

1. The PR from the fork waits, not enqueued, for runner admission
   (tinyland-infra #106); the standing ruling re-enqueues #2, then #3, then
   this PR.
2. The SWB-R27 carrier: a dated TIN-4655 comment carrying the TIN-4668
   read-ACL answer, and the decision-versus-enforcement gate (ADR-0002 →
   Open rulings 1), before P1b.
3. The L0 Host-header canary on honey (SWB-R29), started and stopped by the
   operator.
