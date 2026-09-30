---
title: "Switchboard owned fixture acceptance entrypoint"
date: 2026-09-30
status: active
summary: >-
  Existing store and CLI fixtures now have a bounded-job receipt driver;
  native harness dialog, pod restart and broker-down fallback remain live gates.
refs:
  - TIN-4655
---

Receipt: LAB-TAKEOVER-20260930, SWB-R49, R-N13. Separate source tree starts
at signed integration b5158729355e836a3a98ade90d7649690e7a6900. No design,
registry, service, deployment or process-control behavior is changed.

`just e2e-fixtures-plan` prints exact source/targets/cases without a build.
On Honey or Sting, `just e2e-fixtures /absolute/new/receipt-directory` runs
existing CLI/store tests with one job, one CPU and serial Rust test threads,
ambient rc files disabled and SWB environment removed. Receipt verifies the
six specific Rust test success lines, source HEAD and log digest. No live
broker address is accepted. Fixture temporary SQLite/listeners are already
owned by their existing tests and end normally. The driver adds no lifecycle
control, timeout supervisor, harness startup or telemetry requirement.

This does not replace the live `e2e` gate. Store reopen is not pod-restart
acceptance; synthetic fixture session IDs are not native Claude/Codex/Pi
self-registration. ADR-0001 Identity explicitly keeps Codex notify runtime-owned and routes
registration through swb whoami. The native calling-session binding is a
separate P1b sub-exit, never inferred from process ancestry or fixture IDs.
The registry is intentionally disabled/url null. A separate lab adapter lane
owns opt-in native binding; this fixture changes no Codex surface.

Live v1 still requires custody/backend proof; exact image/source and issued
broker URL; native Neo Claude and Sting Codex/Pi identities and real mailbox
reply/ack; operator-owned named pod-restart gate preserving message/thread
sequence and delivery state; broker-down nonblocking hook and ordinary host
workflow continuation with an explicit degraded receipt; declared registry
endpoint matching the delivered host and active harness profile. Telemetry
qualification follows functional v1 per LAB-TAKEOVER-20260930 and supplies no
substitute for these gates. An operator word is required for each live
restart and harness-start ceremony; agents do not infer it from this fixture.

Validation: plan command and just recipe parsing succeeded; git diff check
and commit-time process-safety audit passed. No Bazel compilation ran on Neo.
Honey heavy admission was held by root at 06:46Z (load31/32cores); Sting's
heavy slot is occupied. Executed Rust-fixture receipt remains pending a
separately approved slot. The fixture driver is ready to run on exact signed
source, not reported as a fixture pass.
