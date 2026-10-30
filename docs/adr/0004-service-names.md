# ADR-0004: stable service names

- **Status:** Proposed. It is not normative. Code needs a TIN-4655 ruling
  first, and the ruling decides the questions under Open rulings.
- **Date:** 2026-10-30, drafted against `main` at `eada83a`.
- **Linear:** TIN-7788, under TIN-4655.
- **Depends on:** opt-in sender binding ([ADR-0003](0003-sender-binding.md),
  SWB-R66 to SWB-R68) and the claim rules SWB-R16, SWB-R60 and SWB-R61 in
  [ADR-0001](0001-agent-switchboard.md).

Line references below are to `eada83a`.

## Context

A producer process runs on several hosts. Each copy must reach whichever
agent session currently serves a named role, such as a reviewer or a
renderer. The serving session changes when it restarts, moves to another
host or hands the role to another session. The producer has no stable
address to send to.

What the broker offers today:

- **Send takes only agent ids.** The store refuses any `from` or `to` that
  is not `harness:host:pid:session_id`
  (`crates/swb-store/src/lib.rs:260-269`, checked in `send_as` at
  `crates/swb-store/src/lib.rs:960-965`). The CLI checks the same shape
  before it calls the broker (`crates/swb/src/main.rs:32`, `:624-631` and
  `:697-700`). Send does not check that the recipient is registered. It
  stores mail for any well-formed id
  (`crates/swb-store/src/lib.rs:1183-1185`).
- **Bound delivery keys survive a restart, not a move.** ADR-0003 names the
  restart problem (`docs/adr/0003-sender-binding.md:22-24`) and fixes it
  with the delivery key `harness:host:session_id`
  (`docs/adr/0003-sender-binding.md:93-97`,
  `crates/swb-store/src/lib.rs:338-346`). The key still carries the harness
  and the host. A role that moves to another host or harness gets a new
  key, and the producer must know both parts in advance.
- **Claims are advisory subject locks.** They record who works on a
  subject and never route mail (`docs/REFERENCE.md:151-156`, SWB-R16). Any
  session may record a non-exclusive claim on any subject, and an orphaned
  claim is never revoked (`docs/REFERENCE.md:179-180`).
- **OTel identity labels telemetry only.** `service.name` is the resource
  attribute `swb-broker`, which names the process that emitted a span
  (`docs/adr/0002-lgtm-plane.md:405-409`). `gen_ai.agent.id` repeats
  `from` (`docs/adr/0002-lgtm-plane.md:426`). Neither is an address, and
  readers must not treat telemetry identity as proof (SWB-R44,
  `docs/adr/0002-lgtm-plane.md:302-305`).

A producer can work around this today. It reads `peers` or `claims`, picks
a session and sends to that exact id. The pick can be stale by the time
the mail lands. Mail sent to a crashed unbound session then waits under a
pid that never reads again.

## Decision (proposed)

1. **A service address.** `to` may be `svc:<name>`, where `<name>` matches
   `[a-z0-9][a-z0-9.-]{0,62}`. `svc` is not a harness kind and the address
   has two parts, so it never parses as an agent id. `from` stays an agent
   id: a session always sends as itself.
2. **Names are configured, not first-come.** `swb serve` reads
   `SWB_SERVICES_FILE`, a JSON file. With the variable unset no name
   exists and the feature is off. An invalid file refuses to start, as an
   invalid `SWB_SEND_BURST` does. Each entry names:
   - `name`;
   - `holders`, a list of `{harness, host, session_id}` patterns, where
     each part is exact or `*`;
   - `secret_sha256`, the SHA-256 of a per-name claim secret that the
     operator provisions out of band;
   - `lease_minutes`, default 15, clamped to 1 to 60.

   ```json
   {"services": [{"name": "reviewer",
     "holders": [{"harness": "claude", "host": "*", "session_id": "reviewer"}],
     "secret_sha256": "<64 hex>", "lease_minutes": 15}]}
   ```

   A send to a name that is not configured answers 400 `unknown_service`,
   so a typo fails at once.
3. **Holding a name is an exclusive claim on the subject `svc:<name>`.**
   It reuses `claim`, `release`, `handoff` and `claims`, with these extra
   rules inside the `svc:` subject namespace only:
   - The claimant must have a live binding (ADR-0003), match one of the
     name's `holders` patterns and present `service_secret`. A claim that
     fails any of these answers 403 `service_unauthorized` and records
     nothing.
   - The claim is always exclusive. A non-exclusive request is refused the
     same way, so no session can block a name with an advisory claim.
   - A second claimant gets the existing refusal with `held_by` (SWB-R60).
     There is no forced takeover.
   - The lease is `lease_minutes`, not `lease_hours`. Only the holder's
     repeat claim, which carries its session token, renews it (SWB-R60
     renewal). `heartbeat` takes no token, so it does not renew a name.
     The client renews at half the lease.
   - The name is free when its claim expires, is released, is handed off,
     or its holder's session ends or its binding lapses. `end` already
     releases every claim (SWB-R60).
   - The client reads the secret from
     `$SWB_STATE_DIR/services/<name>.secret`, mode 0600, before each claim.
     The broker stores only the hash. The secret is never logged, printed
     or put on a command line.
4. **Resolution happens at delivery time.** A send to `svc:<name>` stores
   the message with `to` and the delivery key both set to `svc:<name>`. It
   is not tied to any session. `inbox` for a session reads its own mail and
   the mail of every name it holds right now. `ack` of a name's message
   succeeds only for the current holder. Any other session gets 409
   `not_service_holder`, and the message stays unacked.
5. **Mail waits while a name is unheld.** A message to a configured name
   with no holder is stored and kept until its normal expiry (72 hours by
   default, 14 days at most, SWB-R09). The next holder reads it. The send
   result carries `service_held: true|false` so the producer can see that
   nobody holds the name now. Delivery stays at least once: a message the
   last holder fetched but did not ack is delivered again to the next one,
   with a higher `delivery_count`.
6. **Handoff reuses claim handoff.** The holder offers its `svc:<name>`
   claim with `handoff`. The receiver accepts it with
   `claim {me, handoff_id, service_secret}` and must pass rule 3. In one
   transaction the source becomes `handed_off` and the receiver holds the
   name under a new `claim_id`. Unacked mail moves with the name. A
   planned handoff should ack what it finished first, so the receiver does
   not see it again. A refused acceptance leaves the offer open (SWB-R61).
7. **Config changes route, they do not revoke.** No broker path acts on
   another session's claim (AGENTS.md, Product boundary). When the file
   drops a holder pattern, the existing claim stays recorded. The broker
   stops routing the name to it at the next `inbox` and refuses its acks
   for the name. Changing the file needs a broker restart.
8. **Telemetry.** Names are low cardinality, because only configured names
   exist. The attributes use the `swb.` prefix so nobody reads them as the
   OTel `service.name`.

   | Signal | Addition |
   | --- | --- |
   | `swb.send` span | `swb.to` = `svc:<name>`, `swb.to_service` = `<name>`, `swb.to_harness` and `swb.to_host` empty, `swb.service_held` |
   | `swb.fetch` and `swb.ack` spans | `swb.service`, `swb.service_holder` (the resolved agent id), `swb.service_claim_id` |
   | `swb.claim`, `swb.release` and `swb.handoff` spans | unchanged. The subject is `svc:<name>` |
   | Audit lines | `fetch` and `ack` add `service`, `holder` and `claim_id` |
   | Metrics | `swb_service_held{service}` (0 or 1) and `swb_service_pending_messages{service}` |

   Spans and lines are projections. Truth stays in `claims` and `inbox`
   (SWB-R02, SWB-R44).

## Failure modes

| Case | Behaviour | What the producer sees |
| --- | --- | --- |
| Name not configured | Send refused, nothing stored | 400 `unknown_service` |
| No holder | Mail stored and kept until it expires | `service_held: false`, and `swb_service_held` reads 0 |
| Contested claim | The second claimant is refused with `held_by`. The holder keeps the name | Nothing changes |
| Holder crash | The name stays held until its lease ends, at most `lease_minutes`. Mail waits, then goes to the next holder | Delivery is delayed, and fetched but unacked mail arrives again |
| Lease expiry with a live holder | The name is free. The old holder's next ack of a name message gets 409 and its next inbox no longer returns name mail | At-least-once delivery, possibly a duplicate |
| Binding lapse or `end` | The claim is released, as for any claim | As for a crash, without the lease wait |
| Holder dropped from config | Routing stops at once and the claim stays recorded | As for no holder |
| Broker restart | Claims and mail are in the store. Leases keep their wall-clock expiry | Nothing, if the holder renews in time |

## Security

| Threat | Control | Residual risk |
| --- | --- | --- |
| A session sends as the name | `from` must be an agent id. A bound sender needs its token (SWB-R66) | An unbound sender can still claim any unbound id, as today (SWB-R05) |
| A hostile session claims the name | It must match `holders`, be bound and present the secret | Anyone who reads the secret file and can bind a matching key can hold the name. Rotate by changing the hash and restarting |
| A hostile session blocks the name | Advisory and unauthorized claims on `svc:` subjects are refused | A claim on a `svc:` subject recorded before the upgrade blocks until it expires (Migration) |
| A stale holder acks mail it no longer owns | Ack checks the current holder | None beyond a duplicate delivery |
| Replay of a send | SWB-R46 makes a repeated `msg_id` idempotent or a conflict | None |
| Replay of a claim or handoff | Each claim needs the current session token, which every register rotates. A handoff offer is accepted once and closes on release or `end` | A stolen live token works until it rotates, as in ADR-0003 |
| Flood to a name | The per-sender bucket (ADR-0003 Decision 3) applies to sends to a name | A flood that rotates unbound ids is bounded by the network ACL only |
| A hostile holder reads the role's mail | Secret custody and the network ACL | Out of scope. Bodies must never carry a secret (SWB-R19) |

The broker does not read the caller's network identity, so `host` in a
holder pattern is self-asserted. The secret is the control, and the
pattern narrows who may present it.

## Alternatives considered

- **Use bound delivery keys directly.** The producer sends to the agent id
  of a bound key, and ADR-0003 delivers across restarts. This needs no new
  address. It fails the use case: the key carries the host and harness, so
  a move breaks the address and the producer must know where the role
  runs. Binding is also first-come per key, so it gives no authorization.
- **Client-side lookup through `peers` or `claims`.** The producer looks
  up a holder by a naming convention or an advisory claim, then sends to
  its exact id. This works today. It resolves at send time, so a holder
  change between lookup and delivery strands the mail. Every producer
  repeats the lookup rules. An advisory claim proves nothing, and an
  orphaned one is never revoked.
- **A DNS-like registry.** An external name service maps names to agent
  ids, and clients cache the answer with a TTL. Caches go stale, and
  resolution still happens at send time. It adds a second authority beside
  the broker, against SWB-R02, and it needs its own authentication.
- **Separate service verbs and table.** A `services` table with its own
  claim, release and handoff verbs would leave SWB-R16 untouched. It
  duplicates the claim lifecycle, audit lines, spans and the SWB-R60 and
  SWB-R61 cases. Open rulings keeps this choice open.

## Migration and compatibility

- **Additive.** Agent ids, unbound sessions and bound keys behave as in
  v0.5.0. With `SWB_SERVICES_FILE` unset no `svc:` rule applies.
- **Upgrade order.** Upgrade the broker first. An older broker refuses
  `svc:<name>` as an invalid agent id, so a new client fails closed and
  nothing is misdelivered. An older client refuses the address locally at
  `send_payload`. An older client cannot send `service_secret`, so holders
  upgrade before a name is configured.
- **Existing `svc:` claims.** The claim subject charset already allows
  `svc:`. The broker does not touch a claim recorded before the upgrade.
  It keeps advisory semantics until it ends, and it blocks the name while
  it is active. Before configuring a name, check
  `swb claims --subject svc:<name>`.
- **Schema.** `schemas/envelope-v3.schema.json` gains a `service_address`
  definition, and `to` becomes either an agent id or a service address.
  ADR-0001 marks the schema a draft, so this follows the ADR-0003
  precedent of no version change. A validator pinned to the older copy
  rejects the new address.
- **Spec.** `spec/dhall/Broker.dhall` gains the `svc:` claim rules after
  the ruling. QuickCheck properties to add: at most one holder per name,
  no ack by a non-holder and no message lost while a name is unheld
  before it expires.
- **Docs.** `docs/REFERENCE.md` gains a Service names section, and
  SECURITY.md describes the secret file.

## Consequences

- SWB-R16 gains an exception inside the `svc:` namespace. There a claim
  is exclusive, authorized and routes mail, and it can be refused for a
  reason other than `held_by`. Outside it, claims stay advisory.
- `authority` stays `peer` (SWB-R14). Holding a name proves which session
  serves a role, never operator authority.
- `thread` reads by exact agent id. A holder sees name mail through
  `inbox`, and `thread` shows it only if the ruling extends `thread`.
- A crash delays delivery by up to one lease. A shorter lease shortens
  the delay and costs more renewals.

## Open rulings

1. The reserved `svc:` claim namespace, or separate service verbs and a
   table.
2. Whether the per-name secret is required for every name, or only for a
   name whose `holders` contain `*`.
3. The default and bounds of `lease_minutes`.
4. Whether a holder may send as `svc:<name>` so replies reach whoever
   holds the producer's name, or whether that waits for a `reply_to`
   field.
5. Whether `thread` includes name mail for the current holder.
