# Switchboard service level indicators

Authority: R-C263 (operator interview 2026-10-04, TIN-4655 comment
`230af90b`), R-N13. Design: [ADR-0001](../adr/0001-agent-switchboard.md);
metric names and cardinality: [ADR-0002 → Data model](../adr/0002-lgtm-plane.md).

This document defines what the broker measures. It sets **no targets**.
R-C263 orders the work: define the SLIs, measure them on a live broker, then
hold an operator interview to set targets. A number in this file is a
definition, a design constant or a window, never an objective.

**There is no customer SLA.** The switchboard is internal, tailnet-only
coordination tooling for the operator's own agent seats (SWB-R04). Nothing
here is a promise to any external party, and no credit, penalty or
contractual remedy attaches to any indicator.

## Scope and when measurement starts

- The SLIs apply to the commissioned broker: the one-replica StatefulSet
  behind the Tailscale Service `mcp-agents`, at the MagicDNS name Tailscale
  actually issues (SWB-R52). The v0.2.2 broker is live, but nothing scrapes
  its `/metrics` yet, so no SLI data exists. Fixture runs, local integration trees and loopback `spec-live` runs
  are development evidence, not SLI data.
- Measurement starts at the first scrape after the P1b functional exits pass.
  R-C263 sets no minimum amount of data. The targets interview decides
  whether it has enough.
- Hooks are non-blocking by design (SWB-R10: 2 s timeout, always exit 0) and
  fall back to the degraded-mode ladder in ADR-0001. A hook that times out
  never received an accepted send, so it is not a lost message; it shows up
  in availability and latency instead.

## Measurement window

- **Reporting window:** rolling 28 days. This is a proposed default, not
  a ruled value, and the targets interview may change it. It is evaluated on the Mimir copy of the
  `swb_*` series that the single static scrape job writes (ADR-0001 →
  LGTM → Scrape).
- **Diagnostic windows:** 1 h and 24 h, for dashboards and investigation
  only.
- **Planned maintenance is not excluded yet.** Owner-admitted restarts and
  commissioning windows are recorded as Grafana annotations. Whether they
  are excluded from the availability SLI is a question for the targets
  interview.

## What the broker exposes today

The broker serves `/metrics` on its separate metrics listener (`:9090`,
in-cluster only). As of v0.3.0 it emits six series (`Store::metrics` in
`crates/swb-store/src/lib.rs`). The three claim series arrived with P2
claims (R-C950):

| Series | Type | Meaning today |
| --- | --- | --- |
| `swb_messages_total` | counter, no labels | Accepted new sends. Persisted in the SQLite `counters` table, so it survives a restart; an idempotent retry does not increment it. |
| `swb_mailbox_unacked` | gauge, no labels | Messages not `acked` or `expired` whose TTL has not passed. |
| `swb_sessions{state="live"}` | gauge | Sessions not ended with `last_seen` within the last 900 s. |
| `swb_claims_active` | gauge, no labels | Active claims that have not expired. |
| `swb_claim_overlaps` | gauge, no labels | Active claims that share a subject with another holder's active claim. |
| `swb_handoffs_total` | counter, no labels | Handoff offers created. Persisted in the `counters` table; repeating an open offer does not increment it. |

Each successful `register`, new `send` and `ack`, each message an `inbox`
read returns (a `fetch` line with its `delivery_count`), and each recorded
claim, release or handoff also writes one JSON audit line to stdout. A line
has a second-resolution `ts` and never the body (SWB-R19, SWB-R26). Since
v0.3.0 the message lines carry `msg_id`, `thread_id`, `seq`, `in_reply_to`
and `ticket` (ADR-0002 L1-lite), so a log reader can follow one thread from
send through each delivery to ack.

The labelled forms in ADR-0002 (`{from_harness,to_harness,transport}`,
`{harness,host,state}`, `{to_harness}`) and
`swb_session_last_seen_timestamp_seconds` are designed but **not emitted**.
`GET /healthz` (v0.3.0) answers on both listeners and is a liveness check
only. There is no readiness endpoint, no latency histogram, and no counter
for acks or expiries.

## SLI 1: delivery latency (p50, p99)

- **Definition:** for each message the broker accepted, the time from the
  accepted `send` commit to the **first** `inbox` response that returns it
  (state `fetched`, `delivery_count` 1). Redeliveries are excluded. Report
  p50 and p99 over the window.
- **What it includes:** delivery is pull-based (`inbox`, long-poll up to
  25 s, rechecked every 250 ms), so the number includes the recipient's own
  polling cadence and push-notice reaction (SWB-R17). That is intended: it is
  the latency an agent experiences. Report it per recipient harness, so a
  harness that polls rarely (Codex heartbeats only through tool calls) does
  not mask the others.
- **Unfetched messages:** a message that expires before any fetch has no
  latency sample. Count it separately (SLI 2 accounting), never as a
  latency of TTL.
- **Measured today:** not measurable. The `messages` table stores
  `created_at` in whole seconds and records no fetch time. Since v0.3.0 the
  stdout audit has a `fetch` line per delivery, but its `ts` is whole
  seconds too, so it cannot resolve a 250 ms poll loop.
- **Needed:** a `first_fetched_at` column set in the `inbox` transaction when
  `delivery_count` goes 0 → 1; millisecond timestamps for `created_at` and
  `first_fetched_at` (second resolution cannot resolve a 250 ms poll loop);
  and a histogram `swb_delivery_latency_seconds{to_harness}` observed at
  that transition. A histogram resets on restart; Prometheus `rate()`
  handles that.

## SLI 2: lost acknowledged messages (must be 0)

- **Definition:** an *acknowledged* message is one for which the broker
  returned a send receipt (REST 200 from `/v1/send` or a successful MCP
  `send` result), including the stored receipt an idempotent retry returns
  (SWB-R46). It is *lost* if, at any point inside its retention, it is
  neither in the store nor accounted for by a designed exit. The designed
  exits are `acked` by its recipient and `expired` by its TTL (72 h default,
  14 days maximum, SWB-R09).
- **Why it must be 0:** delivery is at-least-once with receiver-side dedupe
  (ADR-0001 → Mailbox). Duplicates are allowed; loss is not. This indicator
  is a count, not a ratio, and R-C263 rules that it must be 0. How a
  non-zero count is handled is for the targets interview.
- **Not loss:** a TTL expiry (counted, reported beside the SLI, and a
  delivery problem rather than a durability one); a send that failed or timed
  out before a receipt was returned (the client retries with the same
  `msg_id`); a hook that gave up under SWB-R10.
- **Conservation check:** pruning deletes rows, so a row count cannot prove
  this. Persisted counters can:

  ```text
  lost = messages_accepted_total - (acked_total + expired_total + unacked_current)
  ```

  Unacked bodies are pruned at 30 days, after the 14 day maximum TTL, so
  every accepted message reaches `acked` or `expired` before its row can
  disappear.

  `unacked_current` here counts every stored row that is neither `acked`
  nor already counted in `expired_total`, with no TTL filter. It is not
  today's `swb_mailbox_unacked`, which drops a row as soon as its TTL
  passes. With that gauge in the identity, a message past its TTL but not
  yet counted as expired would read as lost until the next expiry pass.
- **Measured today:** only the first and last terms exist
  (`swb_messages_total`, `swb_mailbox_unacked`). Expiry is only written to
  the row during an `inbox` pass, while the gauge filters on `expires_at`,
  so an expired message leaves the gauge without any counter recording it.
- **Needed:** persisted counters `acked_total` and `expired_total` in the
  `counters` table, incremented in the same transaction as the state change.
  Expiry must be counted on a TTL transition independent of mailbox traffic
  (the hourly prune pass already runs without traffic). Export them as
  `swb_messages_acked_total` and `swb_messages_expired_total`, plus
  `swb_messages_lost` computed from the identity above.
- **Black-box check:** a synthetic canary session on a tailnet vantage sends
  to itself with a fresh ULID, fetches, acks and confirms the receipt. It
  also checks across each owner-admitted pod restart that every receipt it
  holds is still fetchable or acked. That is the P1b "durable inbox after
  restart" exit, run continuously.
- **Backup restore counts.** A Litestream restore that rewinds past an
  acknowledged send loses that message, so restore drills (TIN-5105) report
  replication lag at the restore point. Exporting that lag is a custody-lane
  gap, owned by blahaj.

## SLI 3: broker availability on the tailnet

- **Probe definition:** from a tailnet vantage outside the broker's own pod
  and node, at a proposed 30 s cadence (not ruled), `GET http://<issued-MagicDNS>:8080/v1/peers` with no
  `me` parameter, a 2 s timeout (the SWB-R10 hook budget) and the exact Host
  header the route allowlists. Success is HTTP 200 with a JSON body that has
  a `peers` array.
  - Omitting `me` makes this passive discovery: it renews no lease and
    writes nothing (ADR-0001 → Lease), so the probe cannot distort SLI 4.
  - It exercises the tailnet route, the Service, the HTTP listener and a
    SQLite read under the store lock. A wedged store fails it, which a TCP
    check would miss.
  - Probe the issued MagicDNS name, never the
    `agents.ephemera.tinyland.dev` alias. The alias waits for separate route
    proof (SWB-R52), and an alias failure is a DNS-map fault, reported
    separately.
- **SLI:** successful probes divided by total probes over the window. Also
  record the probe's latency distribution: a slow 200 is a latency signal,
  not an outage.
- **MCP path:** the hooks use `/v1/*`, but MCP clients use `/mcp`. A second
  probe sends `initialize` and `tools/list` to `/mcp` (the
  `rmcp_streamable_http_initialize_and_tools` shape) on the same cadence and
  reports separately, so an rmcp-only fault shows up.
- **Measured today:** nothing. The `/metrics` listener is in-cluster, so a
  scrape `up` cannot show tailnet reachability.
- **Needed:** an owner and a host for the black-box prober (a blackbox
  exporter job or an equivalent tailnet-attached probe). Its series go
  through the existing single scrape writer. Alert rules stay blahaj's
  (prometheus-mail, SWB-R28); this repo authors none.

## SLI 4: lease staleness against the 900 s session lease

The lease is 900 s (`register` returns `lease_seconds: 900`). A session
reads `live` while `last_seen` is within 900 s, `idle` up to 6 h, then
`gone`. `last_seen` is renewed by `register`, `peers` with `me`, `send`
(sender), `inbox` and `ack`, and by the Claude/Kimi hooks through those
calls. At `08de158` the broker stamps it from SQLite `unixepoch()`. The R-C262
clock seam replaces that with an injected clock that reads the same host
time. Either way the broker uses one clock, so clock skew between hosts
does not enter.

Two failure directions, measured separately:

- **False live (stale lease):** a session that has exited without
  `SessionEnd` still reads `live`. By construction this lasts at most 900 s
  after its last heartbeat. The SLI checks that the bound holds in practice:
  the distribution of (time `live` ended) − (last heartbeat), which should
  never exceed 900 s plus one scrape interval. Also count the sessions that
  reach `idle` or `gone` without `ended`: these are exits the broker learned
  about only through lease expiry.
- **False idle (premature expiry):** a running session goes more than 900 s
  between heartbeats and reads `idle` while alive. This is the realistic
  risk for Codex, which heartbeats only through tool calls (ADR-0001 →
  Lease). The SLI is the distribution of heartbeat gaps per harness: the
  share of gaps above 900 s among sessions that later renewed is the
  false-idle rate.
- **Measured today:** only the `live` count. Nothing exposes heartbeat gaps,
  per-harness state or lease-expiry exits.
- **Needed:** `swb_sessions{harness,host,state}` and
  `swb_session_last_seen_timestamp_seconds{harness,host}` as designed in
  ADR-0002; a histogram `swb_lease_renewal_gap_seconds{harness}` observed
  when `last_seen` is renewed, using the previous value; and a counter
  `swb_sessions_lapsed_total{harness}` for sessions crossing 900 s without
  `ended`. Testing the transitions in properties needs the injectable clock
  that R-C262 assigns to the next approved release.

## Instrumentation gaps

Each item needs its own reviewed PR. Changes to `swb-store` or `swb-broker`
are protected release inputs, so they reach a live broker only through a new
approved image (R-C262 and the SWB-R53 boundary in
[PRODUCTIONIZATION.md](PRODUCTIONIZATION.md)).

- [ ] `first_fetched_at` column plus millisecond `created_at`, set in the
      `inbox` transaction on the first fetch (SLI 1).
- [ ] `swb_delivery_latency_seconds{to_harness}` histogram (SLI 1).
- [ ] Persisted `acked_total` and `expired_total` counters, changed in the
      same transaction as the state change (SLI 2).
- [ ] TTL expiry counted independent of `inbox` traffic, in the hourly
      prune pass (SLI 2).
- [ ] `swb_messages_acked_total`, `swb_messages_expired_total` and
      `swb_messages_lost` exported (SLI 2).
- [ ] Labelled `swb_messages_total{from_harness,to_harness,transport}` and
      `swb_mailbox_unacked{to_harness}` from ADR-0002 (SLIs 1 and 2 per
      harness).
- [ ] Synthetic canary session: self-send, fetch, ack, and receipt
      re-verification across owner-admitted restarts (SLI 2).
- [ ] Litestream replication-lag export and restore-point lag in restore
      drills; blahaj custody lane, TIN-5105 (SLI 2).
- [ ] Black-box tailnet prober for `/v1/peers` and `/mcp`, with a named
      owner and host (SLI 3).
- [ ] `swb_sessions{harness,host,state}` and
      `swb_session_last_seen_timestamp_seconds{harness,host}` (SLI 4).
- [ ] `swb_lease_renewal_gap_seconds{harness}` histogram and
      `swb_sessions_lapsed_total{harness}` counter (SLI 4).
- [ ] Injectable clock in `swb-store` (R-C262) so properties and tests can
      drive expiry, idle and gone (SLI 2 and SLI 4 tests).
- [ ] Grafana annotations for owner-admitted restarts and commissioning
      windows (measurement window).
- [ ] Targets interview once there is live data (R-C263).
