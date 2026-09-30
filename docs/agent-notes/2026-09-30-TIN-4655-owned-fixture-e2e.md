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
self-registration. Sibling current CLI has no Codex notify event adapter;
lab exact #1947 e6efc979/#1963 4909c8f0 named consolidation 9b17b5f6 has no
Codex swb/notify producer. The registry is intentionally disabled/url null.

Live v1 still requires custody/backend proof; exact image/source and issued
broker URL; native Neo Claude and Sting Codex/Pi identities and real mailbox
reply/ack; operator-owned named pod-restart gate preserving message/thread
sequence and delivery state; broker-down nonblocking hook and ordinary host
workflow continuation with an explicit degraded receipt; declared registry
endpoint matching the delivered host and active harness profile. Telemetry
qualification follows functional v1 per LAB-TAKEOVER-20260930 and supplies no
substitute for these gates. An operator word is required for each live
restart and harness-start ceremony; agents do not infer it from this fixture.
