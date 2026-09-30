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

Root review follow-up (LAB-TAKEOVER-20260930/R-N13): post-run tracked and
untracked source cleanliness is required even when HEAD remains identical.
Bazel testlog-discovery errors now retain a failed receipt. Three mock cases
cover clean completion, same-HEAD source dirt and testlog-query failure; all
pass without builds. Every execution lowers its inherited per-process address
space limit to at most 6 GiB; the unique run output-base uses batch mode and a
512 MiB JVM heap, avoiding any shared server. These are per-process bounds,
not an aggregate cgroup limit. One-job/one-CPU execution remains owner-admitted.

The first mock run used Linux's numeric RLIM_INFINITY on Darwin, causing one
assertion failure; the mock now uses the executing platform's resource
constant. Corrected three-case run passed. This was test fixture portability,
not a runtime-limit change; the initial commit preserved the failed attempt.

First actual attempt at exact signed33bc253 on Honey ended naturally with a
failed receipt at07:02:25Z: test exit2 rejected old --local_cpu_resources;
following info exit134 reported native std::bad_alloc. No fixture ran and
no root cause beyond that native allocation failure was established. The
Bazel9.2 ExecutionOptions source declares --local_resources=cpu=1; the driver
now uses it, caps JVM visible processors1 and thread stacks512KiB in addition
to the existing512MiB heap and6GiB address-space bound. A failed test command
no longer starts a second JVM for info. Root authorized this source-supported
correction and a new bounded attempt; no command spelling evades a guard.

Source references: https://raw.githubusercontent.com/bazelbuild/bazel/9.2.0/src/main/java/com/google/devtools/build/lib/exec/ExecutionOptions.java
(Bazel local_resources option, lines308–347). Java ActiveProcessorCount and
-Xss are standard supported JVM resource options; allocation cause remains
unproved and these settings do not raise the memory ceiling.

Second actual attempt a968969 ended07:05:58Z exit33 during package loading:
globbing-pool pthread_create returned EAGAIN. No Rust fixture ran. Bazel9.2
PackageOptions names legacy_globbing_threads with default100, independent of
JVM visible processors. Source correction now sets legacy_globbing_threads1
and loading_phase_threads1 as well as action/test limits. Memory ceiling
is unchanged. Raw receipt and bazel log copies from both attempts were
preserved under ~/.claude/agent-notes-rescue/2026-09-30/swb-e2e-<sha>/ on Neo.
The second clone's fresh post-attempt HEAD remained a968969 and tracked/
untracked status was clean; failure receipt false source fields mean the
postcheck was not reached, not established source mutation. Heavy execution
remains held pending root's fresh release; no new attempt followed this edit.

Globbing flag reference: https://raw.githubusercontent.com/bazelbuild/bazel/9.2.0/src/main/java/com/google/devtools/build/lib/pkgcache/PackageOptions.java
(lines132–143). Supported JVM resource options: https://docs.oracle.com/en/java/javase/21/docs/specs/man/java.html

Third actual attempt exact51ba55875db77afc4d64d33a2910177d0c263681
started07:12:55Z after root health clearance and ended naturally07:13:25Z
exit33. Analysis reached322packages/10061targets, then native thread creation
failed at Non-eager Symlink planter50. No Rust fixture executed. Official
Bazel9.2 IncrementalPackageRoots has fixed symlink parallelism200, so the
globbing/loading limits did not bound this separate pool. No further heavy
execution is authorized pending root capacity decision.

Admission lapse: the fresh probe immediately before this third launch showed
load52.67/38.68/32.24, CPU PSI some40.87%, full0%, available memory101872184kB,
materially changed from root's earlier healthy sample. I launched instead of
pausing for the changed pressure. Root was notified promptly. Future launches
must evaluate the fresh probe as a separate conditional decision before
spawning, and pause when it exceeds the admitted envelope. No signals,
deadlines, shared-server intervention, or live harness action occurred.

Third raw receipt/log preserved in the same durable rescue tree under
swb-e2e-51ba558; logSHA256
528f9f00aeeae57dbf81942e7184ec355241352065f3d953b6c3e65dcb157260.
Fresh post-run tracked/untracked status was clean. Tool handle12594 ended
naturally and the shared heavy lock was released; the queued devcontainer
peer was informed. Authority LAB-TAKEOVER-20260930/SWB-R49/R-N13.

Pool source: https://raw.githubusercontent.com/bazelbuild/bazel/9.2.0/src/main/java/com/google/devtools/build/lib/skyframe/IncrementalPackageRoots.java
(lines60–108).

Allocator diagnosis authorized separately, not a Bazel retry. Honey glibc2.39;
RLIMIT_NPROC898890; user cgroup pids.max593267/current558/events.max0;
SSH scope PID maximum unlimited/current9. Observed user130processes/1072threads
and kernel thread maximum1797781 do not show PID-limit pressure at sampling.
These are later metadata, not historical proof of the failed JVM's limits.
No hs_err numerical resource receipt was found. No raw environment exported.

Prepared Linux-only small diagnostic compares two sequential processes:
default glibc arena selection versus process-start MALLOC_ARENA_MAX2, oneCPU
affinity,6GiB AS,128threads maximum,512KiB stacks, one4KiB libc malloc/free
per thread. Owned Event release and join in finally; no deadlines or signals.
Sequential startup/ready wait ensures each allocation completed before the
numeric snapshot; it does not simulate JVM allocation patterns or malloc
contention, so a difference supports mechanism plausibility rather than
proving the earlier Bazel failure. No compilation or broker action.
Authority LAB-TAKEOVER-20260930/SWB-R49/R-N13; root explicitly authorized
this tiny diagnostic only, with Bazel retry still held. Linux gate refused
on Neo before output creation; Python syntax check passed.
Official contract: https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html
(M_ARENA_MAX/startup environment parameter);
https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html
(glibc.malloc.arena_max supersedes MALLOC_ARENA_MAX).
