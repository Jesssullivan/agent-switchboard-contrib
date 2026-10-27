# ADR-0003: opt-in sender binding (draft)

- **Status:** Draft. Not accepted and not normative. It records the second
  pull request of the TIN-4655 harness lane so the design can be reviewed.
  It needs a dated TIN-4655 ruling before any code lands, and that ruling
  would amend SWB-R05 and SWB-R14 in [ADR-0001](0001-agent-switchboard.md).
- **Date:** 2026-10-27
- **Linear:** TIN-4655
- **Depends on:** the harness kinds `hermes` and `kimicode` and
  `swb channel --tools full`, which pin `from` and `me` in one client
  process only.

## Context

Identity is self-asserted (SWB-R05). `send` does not check that the sender
is registered, and anyone who can register can re-register any agent id
([SECURITY.md](../../SECURITY.md)). `swb channel --tools full` refuses a
call that names another id, but that guard lives in the client: a second
client on the same network can still send as any id.

A long-running service identity, such as a Hermes agent, also loses its
unread mail when it restarts, because a new process gets a new pid and so a
new agent id.

## Proposal

1. **Opt-in binding.** `register` with `bind: true` mints a session token
   beside the end token. For a bound id, `send`, `inbox`, `ack`, `claim`,
   `release`, `handoff` and `end` need that token. An unbound id behaves as
   it does in v0.3.0.
2. **Per-sender rate limit.** A token bucket per sender answers 429 with
   code `rate_limited` when it is empty.
3. **Stable delivery for bound service ids.** Inbox delivery for a bound id
   keys on (harness, host, session_id) rather than the full agent id, so a
   restart keeps the unread mail.
4. **Release.** Ship as v0.4.0, with the broker and every client upgraded
   together.

## Consequences

- `authority` stays `peer` on every message (SWB-R14). Binding proves which
  registration sent a message. It never grants operator authority.
- Lab's mux agents group stays read-only (peers, claims and thread) until
  this lands, because a mux caller can pass any `from` or `me`.
- Acceptance includes a spoof test: a `send` with `from` set to a bound id
  and no token fails.

## Open questions

- Where a bound client keeps its token (memory or a 0600 file) and how it
  rotates on re-register.
- The bucket size and refill rate.
- Whether `peers` shows that an id is bound.
