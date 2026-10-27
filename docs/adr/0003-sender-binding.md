# ADR-0003: opt-in sender binding

- **Status:** Accepted 2026-10-27 by operator rulings SWB-R66, SWB-R67 and
  SWB-R68 (Linear TIN-4655 comment `0753094f`). It ships as v0.5.0, because
  v0.4.0 shipped without it. It amends SWB-R05 and SWB-R14 in
  [ADR-0001](0001-agent-switchboard.md) as described under Decision.
- **Date:** 2026-10-27 (drafted with agent-switchboard #50 at `bd6f88b`;
  accepted and implemented the same day)
- **Linear:** TIN-4655
- **Depends on:** the harness kinds `hermes` and `kimicode` and
  `swb channel --tools full` (SWB-R63), which pin `from` and `me` in one
  client process only.

## Context

Identity is self-asserted (SWB-R05). `send` does not check that the sender
is registered, and anyone who can register can re-register any agent id
([SECURITY.md](../../SECURITY.md)). `swb channel --tools full` refuses a
call that names another id, but that guard lives in the client: a second
client on the same network can still send as any id.

A long-running service identity, such as a Hermes agent, also loses its
unread mail when it restarts, because a new process gets a new pid and so a
new agent id.

## Rulings

| ID | Ruling |
| --- | --- |
| SWB-R66 | This ADR is accepted and ships as v0.5.0. Binding stays opt-in. Acceptance includes the spoof test: a send with `from` set to a bound id and no token fails. It amends SWB-R05 and SWB-R14 as described here, and `authority` stays `peer`. |
| SWB-R67 | A bound client keeps its token in a 0600 file and rotates it on every re-register. |
| SWB-R68 | `swb peers` shows whether an id is bound. |

The ruling left the bucket size and refill rate to the implementing PR. The
defaults below are that choice.

## Decision

1. **Opt-in binding.** `register` with `bind: true` binds the session's
   delivery key, `harness:host:session_id` (the agent id without its pid),
   and returns a `session_token` beside the end token. The result of every
   `register` says `bound`.
   - While a key is bound, `send` (by `from`), `inbox`, `ack`, `claim`,
     `release`, `handoff` and `end` (by `me`) need that token in the
     `session_token` field. A request without it, or with another value,
     answers HTTP 401 with code `unauthorized` and changes nothing. Over MCP
     it is a tool error with the same body.
   - Only the binding's own agent id may act. An earlier pid of the same
     key is refused even with the token
     (`agent id superseded by a bound registration of its session`).
   - A new registration of a bound key, with any pid and with or without
     `bind`, must present the current token. It keeps the binding and
     rotates the token, which invalidates the old one.
   - An id whose key has no live binding behaves as in v0.4.0, and a token
     it presents is ignored.
   - `peers`, `claims`, `heartbeat` and `thread` take no token.
2. **When a binding lapses.** A binding is live while the latest
   registration of its key asked for it, is not ended and is not gone. It
   lapses when the session ends (`end` with both tokens) or after 6 hours
   without a call (the `gone` band). A lapsed key registers like an
   unbound one. This is the recovery path for a lost token file: wait for
   the lapse or use another session id.
3. **Per-sender rate limit.** Every `send` takes one token from a bucket
   per sender agent id, after the binding check, so a caller without the
   session token cannot drain a bound sender's bucket. An empty bucket
   answers HTTP 429 with code `rate_limited` and
   `rate limited; retry in <n> s`, and stores nothing.
   - Defaults: a burst of 20 sends and a refill of 30 per minute (one every
     2 seconds). An agent dialog sends a few messages a minute, so the
     defaults stop a loop or a flood without touching normal use.
   - `swb serve` reads `SWB_SEND_BURST` and `SWB_SEND_REFILL_PER_MINUTE`
     (whole numbers up to 100000). `SWB_SEND_BURST=0` turns the limit off.
     Another value refuses to start.
   - The limit applies to every sender, bound or not. Buckets live in
     broker memory, so a broker restart refills them.
4. **Stable delivery for bound service ids.** Each stored message records
   its recipient's delivery key. `inbox` and `ack` for a bound agent id
   read every message addressed to any pid of its key, so a restart under a
   new pid keeps the unread mail. An unbound id reads by its exact agent id,
   as before. An existing store gains the column and fills it at startup.
5. **Client token storage (SWB-R67).** `SWB_BIND=1` makes `swb whoami
   --register` and the `swb hook` register ask for a binding. The client
   keeps the session token in
   `$SWB_STATE_DIR/bindings/<harness>:<host>:<session_id>.token`
   (`SWB_STATE_DIR` defaults to `$XDG_STATE_HOME/swb`, then
   `$HOME/.local/state/swb`). The directory is 0700 and the file 0600,
   written to a temporary name and renamed over the old file.
   - The file name has no pid, so a restarted process finds it.
   - Every register presents the saved token and saves the rotated one the
     broker returns.
   - A register that comes back `bound: false` removes the file, and so
     does a SessionEnd that ends the session.
   - `swb send`, `ack`, `inbox`, `claim`, `release`, `handoff`, SessionEnd
     and `swb channel` (both tool sets) read the file before each call and
     send the token. A `401` is retried once when the file changed since
     the read, which covers a hook rotating the token concurrently.
   - The token is never printed, logged or put on a command line.
     `swb whoami --register` prints `session_token_file` in its place.
     These files are not pruned by age, because a service may be idle for
     days.
6. **Visibility (SWB-R68).** Each `peers` row carries `bound`, true while
   that row's binding is live. `swb peers` and the full channel's `peers`
   tool print it.
7. **Release.** v0.5.0, with the broker and every client upgraded together.
   A v0.4.0 broker ignores `bind`, so a v0.5.0 client against it stays
   unbound. A v0.4.0 client against a v0.5.0 broker can use only unbound
   ids.

## Consequences

- **SWB-R05, amended.** Identity stays self-asserted for unbound ids. A
  bound id proves which registration acts as it: the broker refuses a
  `send`, `inbox`, `ack`, `claim`, `release`, `handoff` or `end` for it
  without its session token.
- **SWB-R14, amended.** `authority` stays `peer` on every message. Binding
  proves which registration sent a message. It never grants operator
  authority, and `operator_directed` stays the sender's claim.
- The token is a bearer secret held by the client. Anyone who reads the
  0600 file, or the MCP `register` result that carries it into a model's
  context, can act as the bound id.
- `thread`, `heartbeat`, `peers` with `me` and `claims` with `me` stay
  unauthenticated for every id. A caller can read a bound id's threads and
  keep its lease fresh, which also delays the 6-hour lapse.
- `thread` reads by the exact agent id, so after a restart a bound service
  sees through `thread` only the messages of its new pid; `inbox` still
  delivers the earlier ones.
- Anyone can spend an unbound sender's send bucket by sending as it. A
  bound sender's bucket is spent only after the token check.
- Lab's mux agents group stays read-only (peers, claims and thread) until
  its callers bind, because a mux caller can pass any `from` or `me`.
- The spoof test is
  `spoofed_send_as_a_bound_id_fails_over_rest_and_mcp` in
  `crates/swb-broker/src/lib.rs`, with
  `a_bound_registration_needs_its_session_token_and_rotates_it` in
  `crates/swb-store/src/lib.rs`.
