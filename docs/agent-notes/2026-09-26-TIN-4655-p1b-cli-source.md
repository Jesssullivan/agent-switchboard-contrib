---
title: "P1b CLI source and session-binding boundary"
date: 2026-09-26
status: active
summary: >-
  swb gains bounded REST-backed hook, whoami and inbox commands; identity sources and live broker reach remain separate P1b gates.
refs:
  - TIN-4655
  - SWB-R10
  - SWB-R14
  - R-N11
  - R-N13
---

## Source receipt

R-N13, SWB-R10 and SWB-R14: swb whoami registers an explicitly supplied
harness/session identity; swb inbox reads its explicit SWB_AGENT_ID; the
Claude/Kimi hook reads bounded JSON input and registers on SessionStart,
UserPromptSubmit and Stop. UserPromptSubmit returns a body-free unread notice
as Claude hook additionalContext. SessionEnd calls /v1/end, which checks
the exact proc_start before setting ended. All hook errors remain silent
and exit zero. The lab wrapper applies the outer 1.8-second deadline; each
network call has its own 1.5-second bound. No process is signaled (R-N11).

This source requires explicit SWB_BROKER_URL (HTTP host and port),
SWB_HOST, SWB_SESSION_PID, and SWB_PROC_START. whoami also requires
SWB_HARNESS and SWB_SESSION_ID; inbox requires SWB_AGENT_ID. It does
not infer a parent PID. No live hostname is in source. The current lab hook
projection does not yet supply a proved PID/proc_start, so activation remains
gated. Native Pi/Junie session-file discovery and the Codex calling-session
binding in ADR-0001 are not claimed by this explicit-input source.

## Verification

On Honey, just lock regenerated Cargo, crate-universe and Bazel module
locks as one set. Its last git status command exited 128 because the remote
scratch copy intentionally omits .git; all lock generation commands had
completed, and the three files were copied back together. Honey
just remote-check then passed all eight Bazel targets, including loopback
HTTP registration, request-path rejection and ended transition tests.
Honey just build passed //:build and //deploy:swb_layer.
Neo performed no compile. No live broker, host delivery or process control
was attempted.

## 2026-09-26 deadline correction

R-N13 and SWB-R10: the first CLI source used a synchronous resolver before
starting its socket deadline, so DNS could exceed 1.5 seconds. A follow-up
places the entire DNS/connect/write/read operation in a process-local worker
with a timed result channel. The hook has an independent 1.8-second aggregate
deadline that includes stdin reading and both possible broker calls. A late
worker result is ignored; no process is signaled. This removes the need for
an external shell timeout in lab's staged wrapper once that wrapper is updated.

Honey just remote-check passed all eight Bazel targets after the correction.
The new tests simulate stalled resolution and withhold an HTTP response from
an owned loopback fixture; in both cases the caller returned at the short
test deadline. The store accepts any nonempty proc_start token, so an OS
process-start token from a managed launcher needs no format change. The
identity source and live URL gates above still apply.

## 2026-09-26 review corrections

R-N13 and SWB-R09/SWB-R10/SWB-R14: a CLI request for 20 maximum-size
messages exceeded the client's fixed response cap and could suppress the
hook's unread notice. The CLI now requests one message per page, describes
the hook notice as at least one unread message, and bounds a page at 512
KiB. A regression constructs an actual stored message with a 16 KiB body
and twenty 2 KiB artifact references made of JSON-escaped control bytes.
Its serialized inbox response exceeds the old 128 KiB cap and fits the new
one. The broker's inbox semantics and its 100-message maximum stay intact.

R-N13 and SWB-R09/SWB-R14: each thread now has a transactional high-water
counter in SQLite, so pruning an old acked latest message cannot reuse its
sequence. On an existing store, initialization seeds counters from retained
messages. Successful send, inbox and ack calls refresh only the caller's
registered lease; a send never refreshes its recipient. An exact idempotent
retry also refreshes its sender.

R-N13 and SWB-R14/SWB-R33: send constructs an allowlisted envelope instead
of copying arbitrary request fields. Unknown keys such as state are rejected,
optional fields are type-checked, and the broker still stamps authority peer.
The audit regression checks that neither a message body nor an unapproved
state reaches its stdout record. Honey just remote-check passed all eight
Bazel targets after these changes, and just build passed the binary and
image layer. No host deployment or process control occurred.
