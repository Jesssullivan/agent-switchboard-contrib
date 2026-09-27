# ADR-0001: agent-switchboard

- **Status:** Accepted (P0). P0 closed 2026-09-25; P1a (substrate) in
  progress. The R0 revision of 2026-09-25 (this PR) is Proposed until it
  merges through the queue, like [ADR-0002](0002-lgtm-plane.md), which
  holds the LGTM plane itself.
- **Date:** 2026-09-25 (revised 2026-09-25, R0)
- **Linear:** TIN-4655
- **Sources:**
  - the approved plan (plan mode, 2026-09-24/25), whose rulings are
    restated in the TIN-4655 description;
  - TIN-4655 comment "P0 rulings, first round" (`67611936-9782-4ef6-b5d2-71eef3be40cc`,
    2026-09-25T11:38Z);
  - TIN-4655 comment "P0 rulings, second round" (`643df6af-8e88-496e-9562-7d6fa820fab9`,
    2026-09-25T11:40Z);
  - TIN-4655 comment "R0: LGTM rulings interview" (`73f1ce72-28c5-42d6-9039-1135e1c92121`,
    2026-09-25T17:57Z), which reworded SWB-R02, added the rulings recorded
    as SWB-R25 to SWB-R32 in ADR-0002, and adopted the critique fixes folded
    in below;
  - TIN-4655 comment "Open rulings on ADR-0002 / agent-switchboard #4"
    (`f870729e-dbcb-451d-97c3-0c1e1f8e9641`, 2026-09-25T19:11Z), which
    answered ADR-0002's first four Open rulings, recorded there as SWB-R33
    to SWB-R36 and applied below with dated notes.

This ADR is the approved plan with every P0 ruling applied. Where a P0 ruling
changed the draft, the text below states the ruled design and cites the
ruling ID. [Deltas from the draft](#deltas-from-the-draft) lists each change.
[Rulings](#rulings) gives the source and quote for every ID. Where the R0
round changed a line, the earlier text stays in a dated supersession note
next to the new one; nothing is silently rewritten (R-N13).
[Deltas from the R0 LGTM round](#deltas-from-the-r0-lgtm-round-2026-09-25)
lists those changes.

## Context

On 2026-09-24/25, sessions in different harnesses and on different hosts had
to coordinate: the blahaj seat on neo, sting's glorious.build session, and lab
lanes. The only working path was Claude Code's per-session socket
(`/tmp/cc-socks/<PID>.sock`). Across hosts that needs the hand-built SSH
forward skill (lab #1920, `remote-session-message`). The other harnesses have
no inbound surface:

- Pi's RPC reaches only the process that spawned it.
- Junie's `--acp` is unused.
- Codex's daemon socket is unproven for live threads.

Failures seen:

- misrouted answers;
- relayed "operator rulings" that needed confirming;
- stale sockets;
- Kimi being indistinguishable from Claude;
- a Tailscale SSH `-R` socket created root-owned;
- `from=` addresses that don't work across hosts.

The operator asked for a harness-agnostic design, with and without tmux/ssh,
registered once on the tailnet so that new harness instances spawn nothing.

## Decision

### Scope and principles

- **Scope:** discovery, threaded dialog, and task claims with handoff
  (SWB-R01). Claims are advisory by default (SWB-R16).
- **Liveness and LGTM (SWB-R02, reworded 2026-09-25):** "Self-registration
  is authoritative; the broker writes through to LGTM, which is the
  read/query/context plane and never the commit path." Acks, claims,
  sequence and leases stay in the broker. The projection is
  [ADR-0002](0002-lgtm-plane.md).
  - *Superseded wording (P0, 2026-09-25):* "self-registration is the source
    of truth. LGTM is a view." The R0 answer was "Re-word, don't reverse".
- **Delivery:** a mailbox plus native push where it exists, never tmux
  injection (SWB-R03).
- **Hosting:** a blahaj pod, tailnet-only (SWB-R04).
- **Language and identity:** Rust, with self-asserted identity (SWB-R05).
- **Process control:** no tool, hook or daemon acts on a process, a tmux pane
  or another session's claim (R-N11). agentd never unlinks a socket and never
  signals a process.

### Broker (`swb`)

- **Stack:**
  - Rust (rules_rust + crate_universe, Bazel 9), MCP via `rmcp` Streamable HTTP;
  - one binary, `swb serve | agentd | hook <harness> | whoami | inbox`;
  - `serve` exposes `:8080/mcp`, a REST twin `/v1/*` for hooks, and
    `:9090/metrics` inside the cluster.
- **Storage:**
  - SQLite (WAL, `synchronous=FULL`, single writer) on a 1 GiB PVC, in a
    one-replica StatefulSet;
  - a Litestream sidecar replicates to RustFS;
  - it holds coordination state only. Findings stay in repo notes or Linear.
- **Identity:** `harness:host:pid:session_id`, plus `proc_start` to guard
  against PID reuse. How each harness supplies it:
  - Claude and Kimi: the SessionStart hook passes `session_id`, and
    `kimi-claude.sh` exports `SWB_HARNESS=kimi`.
  - Codex: `swb whoami` calls `register`, the same path as Junie, OpenCode
    and Pi. Codex's `notify` is a runtime-owned top-level scalar that
    already carries the computer-use `turn-ended` hook on neo, so Home
    Manager declares no `notify` and no new table for the switchboard
    (critique fix adopted 2026-09-25 under R0 ruling 3; evidence in
    ADR-0002 → Enrollment). A fan-out `notify` wrapper that chains the
    existing program would need its own ruling. **How `swb whoami` binds
    to the calling Codex session is unproven** (no lane evidence): the
    binding must be something Codex hands the tool process — an
    environment variable or a per-session path it exposes — recorded with
    evidence in the P1b receipt, and never derived from pid ancestry
    (the R-N11 corollary). Until then Codex registration is its own P1b
    sub-exit, and the round trip does not silently depend on it.
    - *Superseded (P0):* "Codex: its `notify` hook passes the thread UUID."
    - *Superseded (refutation, 2026-09-25):* "`swb whoami` reads Codex's own
      session records read-only" — a place, not a binding.
  - Junie, OpenCode, Pi: `swb whoami` reads `~/.junie/processes/*.json` and
    `~/.pi/agent/sessions` read-only, then calls `register`.
  - Every tool takes an explicit `me`, because the mcp-mux gateway hides
    Junie's MCP session id.
- **Lease:**
  - Heartbeat sources: the Claude/Kimi hooks (SessionStart, UserPromptSubmit,
    Stop) and every tool call. Codex heartbeats through tool calls only,
    because its `notify` slot is not lab's to declare (see Identity).
    - *Superseded (P0):* "the hooks (SessionStart, UserPromptSubmit, Stop,
      Codex `notify`) and every tool call."
  - States: `live` for 15 minutes, `idle` up to 6 hours, then `gone`.
    SessionEnd sets `ended`.
  - Host observations only corroborate a lease; they never renew it.
- **Mailbox:**
  - ULID `msg_id`, which the client may supply for idempotent retries;
  - *Amended 2026-09-26 (SWB-R46):* `msg_id` is globally
    unique at the broker. A `send` that reuses a stored `msg_id` with the
    same `from` and the same body hash is the idempotent retry and returns
    the stored receipt; one with a different `from` or body is answered
    with an opaque conflict, returns no stored receipt or metadata, and
    stores nothing. Exact retries remain idempotent.
    The global key makes the broker-derived trace id in ADR-0002 safe
    to derive from `msg_id` alone. *Superseded 2026-09-26:* the proposal
    returned the stored receipt even when the sender or body differed;
  - `thread_id` plus a per-thread `seq`, assigned inside the write
    transaction;
  - at-least-once delivery, with a `delivery_count` and receiver-side dedupe;
  - states `queued`, `notified`, `fetched`, then `acked` or `expired`;
  - TTL 72 h by default, 14 days at most. Acked messages are kept 7 days,
    unacked 30 days (SWB-R09).
  - *Added 2026-09-25 (SWB-R25):* those numbers are broker retention only.
    "Tempo and Loki keep 7 days, and the broker keeps 30 days for unacked
    messages." A reader of LGTM treats a missing message as "expired from
    the view or never projected", never as "never sent", and asks `thread`.
- **Claims (SWB-R16):**
  - Advisory by default. `claim` always succeeds and returns `overlaps[]`.
  - A claimant may request `exclusive`. A second `exclusive` claim on the same
    subject returns `held_by` and is not recorded. This ruling is the carrier
    for that one narrow refusal.
  - Non-exclusive claims always succeed.
  - Leases default to 4 h, at most 24 h.
  - When the holder is `gone`, a claim shows as `orphaned`. It is never
    revoked.
  - `handoff` creates a receipt in `offered`. The receiver's claim accepts it
    and marks the source `handed_off`.
  - **Not yet ruled.** A P2 proposal will cover:
    - what an `exclusive` request does while non-exclusive claims already
      exist;
    - whether a non-exclusive claim against an exclusive holder lists that
      holder in `overlaps[]` (proposed: yes, as `held_by`).
- **Linear access (SWB-R15):**
  - The broker may read a ticket's title, state and assignee, to annotate
    claims and overlaps.
  - It may post handoff receipts as Linear comments. This ruling is the
    carrier for those comments.
  - It never moves issue state and never edits descriptions. The estate ban
    on state-writing automation holds.
  - It needs a Linear token held as a blahaj-side sops leaf, scoped as
    narrowly as Linear allows. Other reconciliation stays a manual sweep.
- **Envelope v3 (SWB-R14):**
  - v3 is the lab #1920 v2 envelope (`msg_id`, `from`, `sent_at`, `ticket`,
    `authority`, `reply_to`, `reply_expires`, `reply_format`, `in_reply_to`,
    body), plus `thread_id`, `seq`, `ruling`, `transport` and
    `operator_directed`, with `reply_to: ag:<agent_id>`.
  - The broker writes `authority: peer` on every message, overwriting
    whatever the sender sent. It never emits `operator`.
  - A sender may set `operator_directed: true`, but only with a `ruling`
    pointer (a Linear comment URL or a ruling ID). The flag is shown as the
    sender's claim. Receivers treat the pointer as something to check, never
    as authority.
  - The receiver skill says: confirm any cited ruling with your own operator,
    AGENTS.md or the Linear comment before acting.
  - The draft schema is [`schemas/envelope-v3.schema.json`](../../schemas/envelope-v3.schema.json).
- **Payload limits:**
  - the body is at most 16 KiB, and there are at most 20 artifact refs;
  - `ticket` must match `^TIN-\d+$|none`;
  - control characters are stripped;
  - bodies are returned framed as teammate data and never pass through a
    shell.
- **Audit (SWB-R19; additions 2026-09-25, SWB-R26 and SWB-R27):**
  - one JSON line per mutation, `{ts,op,me,to,ticket,ruling,msg_id,size,body_hmac}`
    **plus the message body**, shipped to Loki by Alloy;
    - *Changed 2026-09-25 (refutation fix, proposed):* the hash field is a
      keyed HMAC under a broker-held key, not a bare `sha256`, because
      Loki and Tempo are readable without credentials today and a bare
      hash confirms a guessed short body (ADR-0002 → Risks);
  - retention and access follow Loki's (7 days, SWB-R25);
  - senders must never put a secret in a body, and the broker does not
    redact. These lines never pass through the TIN-4668 collector
    scrubber (stdout → Alloy → Loki), so tinyland.dev's `loki.process`
    stage for the broker namespace gets a redaction step with the same
    pattern set as that scrubber (SWB-R34, 2026-09-25);
    - *Superseded 2026-09-25:* "whether SWB-R19 accepts that or an
      Alloy-side redaction stage is added is ADR-0002 → Open rulings 4";
  - bodies go only to Loki, "never into Tempo span attributes" (SWB-R26).
    Tempo truncates any attribute at 2048 bytes and the body cap is 16 KiB;
  - the route is broker stdout → Alloy `loki.source.kubernetes` →
    `loki.process`, with a tinyland.dev L1 `loki.process` stage that lifts
    the id keys into structured metadata and `op` into a label (SWB-R26).
    Not the tailnet Loki push, which is reserved for host journald, and not
    OTLP logs, because the collector has no logs pipeline;
  - Bodies stay out of stdout until a named body-read ACL audit is
    decided, including who can read Loki today (recorded in `73f1ce72`
    under the pick "7d in LGTM, ACL decided before P1b (Recommended)").
    That decision comes before P1b (SWB-R27). Until then the audit line
    carries `size` and `body_hmac` only. The operator's answer to the
    shared read-ACL question is on TIN-4668 (`1001c0fb`, "A: tailnet ACL,
    admins + MCP"), and it is SWB-R27's decision (TIN-4655 `f870729e`).
    Bodies ship only after TIN-4670 applies ACL A in
    `Jesssullivan/tailnet-acl` and a raw Loki read from a non-admin
    tailnet node is refused (SWB-R33, "Once ACL A is enforced live
    (Recommended)"). The line schema and the audit's reader scope are in
    [ADR-0002](0002-lgtm-plane.md).
    - *Superseded 2026-09-25:* "carrying it onto TIN-4655 as SWB-R27's
      decision, and whether bodies wait for TIN-4670's enforcement or only
      for the decision, are asked in ADR-0002 → Open rulings 1."
- **Failure behaviour (SWB-R10):** hooks time out after 2 s and always exit 0.
  The broker being down never blocks a harness.

### MCP tools (11)

- The registry key is `agents` and the Junie alias is `ag`. The longest
  composed name, `mcp_mcp-mux_ag__heartbeat`, is 25 characters.
- The tools are `register`, `heartbeat`, `peers`, `send`, `inbox` (long-poll
  up to 25 s), `ack`, `thread`, `claim` (with optional `exclusive`),
  `release`, `handoff` and `claims`.
- None of them acts on a process, a tmux pane or another session's claim
  (R-N11).

### Harness reach

- **Registry (SWB-R11):**
  - `vars/mcp_registry.yml` in lab targets claude_code, codex and opencode.
  - Kimi inherits through `kimi-claude.sh`.
  - VS Code is excluded, because it bridges per session.
  - *Enrollment path, stated 2026-09-25 (critique fix, R0 ruling 3):* for
    Claude and Kimi the entry reaches the harness as `vars/mcp_registry.yml`
    → `export-registries.py` → `tinyland.mcp` → `~/.claude.json`. It never
    goes through `settings.json` `mcpServers`: that binding is a legacy
    escape hatch that warns, and activation prunes registry-managed servers
    out of it. The contract test asserts the `~/.claude.json` projection.
- **Junie (SWB-R12, SWB-R41):** the existing broker path is the mcp-mux
  `tracker` group, never `mux`. P1b also delivers an explicitly selected
  combined broker plus LGTM read path, with its combined tool budget
  measured. Existing default groups stay intact.
- **Pi (SWB-R18, SWB-R37, SWB-R41):** the `agents` profile ships in P1b
  through `pi_mcp_profile_policy`, including registration and a threaded
  round trip. P1b includes an explicit combined broker plus LGTM read
  path with a measured tool budget. Pi's existing `zero` default remains
  until explicit profile selection. Pi can also use `swb inbox` via bash.
- *Superseded 2026-09-26:* the September 25 design left Junie's `tracker`
  and Pi's `agents` sessions without Grafana, left Pi's phase proposed,
  and deferred whether to provide a combined path to the interview.

### Push

- **v1a, everywhere including neo:** the UserPromptSubmit hook returns
  `additionalContext` of the form "N unread from X (TIN-…) — `ag inbox`".
- **v1b:** `swb agentd` runs as a launchd agent on PZM **and neo** (SWB-R17)
  and as a systemd user unit on honey, sting and bumble.
  - It long-polls `/v1/notify?host=` and writes a one-line notice, never the
    body, to a verified `/tmp/cc-socks` socket, at most once per 60 s.
  - On neo it must stay tiny: long-poll only, at most one notice per 60 s, no
    bodies. This respects the neo teletype load budget.
  - It only goes live once Claude's socket framing has been captured from a
    real SendMessage.
  - *Amended 2026-09-26 (SWB-R40, SWB-R47):* agentd may emit `swb`
    telemetry, limited to lifecycle metadata, counts and errors, with no
    message bodies or other content. Its bounded event shape and custody
    remain design work. Collector proof of source identity is required by
    SWB-R44; that telemetry qualification follows v1 functional proof
    (SWB-R48) and does not block the broker MVP. The broker remains the
    authority for consequential state. Census textfiles still require a
    declared writable path; neo has no textfile directory.
    *Superseded 2026-09-26:* the unratified September 25 design said agentd
    never emits OTLP anywhere and the broker is the only `swb` writer.
  - On neo and PZM `/nix` is an external volume, and a `/nix`-hosted
    LaunchDaemon on neo never started (dyld "file system sandbox blocked
    open()", lab `nix/darwin/modules/node-exporter-darwin.nix:14-33` at
    `3d755193`). Before P2 exits, agentd is either staged onto the internal
    volume as node-exporter is, or proven to start from `/nix` in the user
    domain and survive a reboot on both hosts (critique fix, R0 ruling 3).
- **Stale sockets:** a socket is dead if any of these holds:
  - its pid is gone;
  - its process started after the socket's mtime;
  - its executable isn't claude;
  - `connect()` fails.

  agentd never unlinks a socket or signals a process.
- **Codex (SWB-R20):** daemon push waits for a recorded proof that it reaches
  a live thread.

### LGTM (read/query/context plane; the broker writes through)

*Retitled 2026-09-25.* The P0 heading was "LGTM (view, not truth)". The P0
text is kept below where it still holds, with dated notes where it changed.
The plane itself — the Tempo dialog skeleton, the Loki line schema, the
Mimir series, the lookup recipes, the outbox and the L0–L5 phases — is
[ADR-0002](0002-lgtm-plane.md).

- **Broker metrics:**
  - `swb_sessions{harness,host,state}`;
  - `swb_session_last_seen_timestamp_seconds`;
  - `swb_mailbox_unacked`;
  - `swb_push_notices_total`;
  - `swb_claims_active`;
  - `swb_claim_overlaps`;
  - `swb_handoffs_total`.
  - *Added 2026-09-25:* `swb_messages_total{from_harness,to_harness,transport}`
    as the edge source for the node graph, `swb_lgtm_export_failures_total{signal}`
    and the outbox and reconciliation gauges (ADR-0002 → Data model). No
    long-lived series carries a `pid`, `session_id` or `agent_id` label,
    because `agent_id` embeds pid and session (critique fix).
- **Scrape:** a static job in `tinyland.dev/infra/staging/monitoring.yaml`,
  because honey has no ServiceMonitor CRDs. That job is the single writer of
  `swb_*` into Mimir (the registry's double-count rule); Alloy does not also
  scrape the broker.
  - *Superseded 2026-09-25 (SWB-R28):* "Alert rules and dashboards belong to
    blahaj; lab authors none." Now: "Dashboards go to tinyland.dev's owner
    release, and paging alerts to blahaj prometheus-mail." prometheus-mail
    sees `swb_*` through a federation match on the staging Prometheus (the
    TIN-4324 precedent), never through a second scrape. lab still authors
    neither.
- **Census:** agentd writes `agents.prom` to node_exporter's textfile dir
  every 60 s: `swb_local_session{harness,pid,session_id,socket_ok}` and
  `swb_local_stale_sockets`.
  - *Stated 2026-09-25:* these are short-lived series, so `pid` and
    `session_id` are acceptable here and nowhere else. On the Linux voters
    the directory is `/var/lib/node_exporter/textfile`, created by blahaj's
    `honey_hardware` role with `owner`/`group` set to the node_exporter
    user and group and mode `0755`
    (`blahaj@2071613c ansible/roles/honey_hardware/tasks/main.yml:131-138`),
    so a user-level agentd needs a declared writable path; on PZM it is
    `/var/tmp/node_memory_prometheus`
    (`lab@3d755193 nix/darwin/petting-zoo-mini.nix:619,638`, the
    `textfileDirectory` declaration; the Darwin module default is
    `/var/lib/node_exporter/textfile`,
    `nix/darwin/modules/node-exporter-darwin.nix:219-221`); neo publishes
    no textfile files (estate-lgtm lane, `node_textfile_mtime_seconds` by
    job and file) and is skipped.
- **Skill flow:** `ag peers` is the trusted list. A Grafana `query_prometheus`
  join only annotates disagreements, for example "registered, no local
  process" or "local, unregistered → use the SSH skill".
  - *Added 2026-09-25:* readers sort by `seq`, never by Tempo's result
    order; thread enumeration is broker-only (`thread`), Tempo is used per
    message, and a LogQL query for a thread's bodies is a sample that is
    checked against `thread`'s max `seq`; an absence in LGTM means
    "expired from the view or not projected"; and a body that comes back
    from Loki is framed as peer data exactly like one from `inbox`, never
    as an instruction.

### Exposure and auth

- **Service:** a Tailscale operator Service `mcp-agents`, tagged
  `tag:k8s,tag:mcp-proxy` (SWB-R06).
  - It serves plain http on port 8080, because WireGuard already encrypts the
    link and Pi's bridge rejects https.
  - `agents.ephemera.tinyland.dev` is a tailnet-only alias, one blahaj
    `tailnet-dns` map entry.
  - Use whatever hostname Tailscale actually issues, which may be
    `mcp-agents-2`.
- **Auth:** no bearer in v1, because messages carry no authority. If spoofing
  ever matters, add a per-host bearer later.
- **Accepted trade-off:** anything that reaches `tag:k8s:*` can reach the
  broker. `authority: peer` stamping limits the damage. So does the fact that
  claims refuse only exclusive-vs-exclusive: a spoofed exclusive claim can
  block another exclusive claim, never an advisory one.

### Repo and fork convention (new estate standard)

- **Layout:**
  - Bazel: `MODULE.bazel`, `.bazelversion` (9.2.0) and the lock files
    (`MODULE.bazel.lock`, `Cargo.lock`, `cargo-bazel-lock.json`);
  - code: `crates/{swb-proto,swb-store,swb-broker,swb-agentd,swb}` and
    `schemas/`;
  - `deploy/`: the image, via rules_oci in P1b;
  - `docs/{adr,agent-notes}`;
  - tooling: a `flake.nix` devshell and a `justfile` (`check`, `build`,
    `image`, `e2e`, `lock`, `fork-setup`);
  - contracts: `tinyland.repo.json`, `AGENTS.md`, `CLAUDE.md`.
- **CI:**
  - `xoxd-ai/ci-templates` `rust-bazel-application.yml`, pinned by commit, on
    GloriousFlywheel runners (runner group `tinyland-infra`, label
    `tinyland-nix`);
  - it runs for `pull_request`, `merge_group` and `push: main`;
  - `ci-ok` is the one required aggregate check;
  - the image is published by digest from main;
  - the repo is private, as the template requires.
- **Canonical repo:** `xoxd-ai/agent-switchboard`. Its ruleset on `main`
  requires:
  - signed commits;
  - a PR with 0 approvals and the merge method `merge`;
  - the `ci-ok` check from GitHub Actions;
  - the merge queue (MERGE, one entry built at a time, ALLGREEN);
  - no force-push and no deletion.

  It has no bypass actors.
- **Fork (SWB-R08, SWB-R13):** `Jesssullivan/agent-switchboard`, private. It
  requires the xoxd-ai org setting that allows forks of private repositories.
  SWB-R13 authorizes it, and the operator sets it in the GitHub UI (SWB-R23).
- **Remotes:** `origin` is the fork, and agents push only there. `upstream` is
  xoxd-ai, with its push URL set to `DISABLED` (`just fork-setup`).
- **Branches** are named `<type>/tin-####-<slug>-<yyyymmdd>`. PRs go from the
  fork to `upstream/main`.
- **CI on fork PRs (SWB-R13):**
  - The template refuses private runners for fork PRs, and hosted runners are
    forbidden estate-wide.
  - So on a fork PR both CI jobs are skipped, and `ci-ok` reports as passing:
    "gated in merge queue".
  - The full lane runs on `merge_group`, where `ci-ok` must pass for the PR to
    land.
  - `just check` on sting or honey is the pre-queue signal (`just
    remote-check` from neo).
- **Census:** one clone per repo. The fork is recorded in
  `tinyland.repo.json` `contracts.agent_contract` as free text, because the
  manifest schema has no fork field.

### Degraded mode, in order

1. Same-host Claude peer: a direct `uds:` SendMessage with the v3 envelope
   (`transport: direct`).
2. Cross-host Claude peer: the `remote-session-message` skill (lab #1920,
   `transport: ssh-forward`).
3. Any harness: a Linear comment on the ticket, written by the agent itself
   (`transport: linear`).
4. Otherwise: the operator.

## Deltas from the draft

| Area | Approved draft | Ruled design | Ruling |
| --- | --- | --- | --- |
| Authority | The broker never emits `operator`; `operator_directed` is shown as the sender's claim | Unchanged stamping (`authority: peer` always). `operator_directed` now requires a `ruling` pointer, which receivers check and never treat as authority | SWB-R14 |
| Linear | The broker holds no Linear credential and never reads or writes Linear | It reads title, state and assignee, and posts handoff receipts as comments. It never moves state or edits descriptions. Token as a blahaj-side sops leaf | SWB-R15 |
| Claims | Advisory, never refused | Advisory by default. An optional `exclusive` flag makes a second exclusive claim return `held_by` without recording | SWB-R16 |
| Push on neo | neo stays pull-only | agentd also runs on neo as a launchd agent: tiny, long-poll, at most one notice per 60 s, no bodies | SWB-R17 |
| Pi | Deferred to P4, "if ratified" | An `agents` Pi MCP profile ships in v1 | SWB-R18 |
| Audit | Metadata only, no bodies | The audit stream includes bodies. Retention and access follow Loki's | SWB-R19 |
| Retention | Proposed 7 d / 30 d / 72 h | Accepted as proposed | SWB-R09 |
| Hooks | Proposed 2 s, exit 0 | Accepted as proposed | SWB-R10 |
| Registry | Proposed claude_code, codex, opencode; no VS Code | Accepted as proposed | SWB-R11 |
| Junie | Proposed `tracker`, never `mux` | Accepted as proposed | SWB-R12 |
| Forks | Proposed private forks, queue-only CI for fork PRs | Accepted as proposed | SWB-R13 |
| Codex push | Only after a live-thread proof | Unchanged | SWB-R20 |

## Deltas from the R0 LGTM round (2026-09-25)

Source: TIN-4655 comment `73f1ce72`, and for the rows marked SWB-R33 to
SWB-R36 the amendment comment `f870729e` (2026-09-25T19:11Z). The new
ruling IDs are recorded, with their quotes, in
[ADR-0002 → Rulings](0002-lgtm-plane.md#rulings).

| Area | P0 text | R0 design | Ruling |
| --- | --- | --- | --- |
| Source of truth | "self-registration is authoritative; LGTM is a view" | Reworded: the broker writes through; LGTM is the read/query/context plane and never the commit path. Acks, claims, sequence and leases stay in the broker | SWB-R02 (reworded) |
| Retention | 7 d acked / 30 d unacked / TTL 72 h, in the broker | Unchanged in the broker. Tempo and Loki keep 7 d; only the broker keeps 30 d unacked | SWB-R25 |
| Bodies | In Loki, shipped by Alloy | Never Tempo span attributes; broker stdout → Alloy with a tinyland.dev `loki.process` stage | SWB-R26 |
| Bodies, gate | (none) | Out of stdout until a named body-read ACL audit is decided, before P1b | SWB-R27 |
| Phases | P0, P1a, P1b, P2, P3 ("Junie and LGTM"), P4 | L0–L5 interleaved with the broker phases; P3's LGTM items move to L3 | SWB-R28 |
| Dashboards and alerts | Both blahaj's | Dashboards: tinyland.dev owner release. Paging alerts: blahaj prometheus-mail | SWB-R28 |
| L0 gate | (none) | The mcp-grafana bump starts only after a Host-header canary | SWB-R29 |
| ADR revision | (none) | A Fable lane revises ADR-0001 and adds ADR-0002 with every critique fix; Opus refutes; PR through the fork | SWB-R30 |
| Harness-native telemetry | Out of scope | L5: all on, full content, scrubbed at the collector | SWB-R31 |
| Telemetry ingest | (none) | A tailnet OTLP/HTTP endpoint on the existing collector, requested as TIN-4668; lab exporters gated off until **the scrubbed endpoint** (`otlp-harness-http-tailscale`, 4318 → 14318) exists, and never pointed at the retained `otlp-observability-http:4318` (`1001c0fb`) | SWB-R32 |
| agentd (September 25 proposal; superseded by SWB-R40/SWB-R47) | Tiny on neo | Never emits OTLP anywhere; census only; `/nix` exec proof before P2 exits | Design constraint, not ruled (ADR-0002 → Open rulings 1, formerly 5); the `/nix` proof is a critique fix |
| Codex identity | `notify` hook | `swb whoami`; `notify` stays runtime-owned; no new table | Critique fix (R0 ruling 3) |
| Claude/Kimi enrollment | (unstated) | registry → export-registries → `tinyland.mcp` → `~/.claude.json` | Critique fix |
| Claude hooks | (unstated priority) | Merged at `mkDefault`, with a contract test that the three fail-closed guards survive | Critique fix |
| Junie and Pi lookup (September 25 gap; resolved by SWB-R41) | (unstated) | One Junie server, one Pi profile: a switchboard session has no LGTM lookup | Critique fix |
| Projection loss | (n/a) | Transactional outbox plus a reconciliation gauge; "absent" means expired or not projected | Critique fix |
| Lifecycle spans | (n/a) | Each lifecycle step is its own trace, linked back to the send span | Critique fix |
| Thread enumeration | (n/a) | Broker-only | Critique fix |
| Citations | (n/a) | tinyland.dev re-pinned to main `68a16f65` | Critique fix |
| Bodies, gate (amendment) | Out of stdout until the body-read ACL is decided | The TIN-4668 answer "A: tailnet ACL, admins + MCP" is SWB-R27's decision; bodies ship only once TIN-4670 has applied ACL A and a raw Loki read from a non-admin tailnet node is refused | SWB-R33 |
| Bodies, redaction (amendment) | The broker does not redact; no scrubber on the stdout path | A redaction step in the broker namespace's `loki.process` stage, with the TIN-4668 collector scrubber's pattern set | SWB-R34 |
| Harness telemetry in Tempo (amendment) | (unstated; content kept in Loki by design) | Tempo is the harness recency index: tool names, file paths and ticket IDs as span attributes; bodies and full content stay in Loki | SWB-R35 |
| L5 timing (amendment) | Last, after P4 (the adopted order) | Independent of P4; starts once the scrubbed endpoint, the scrubber and the enforced ACL all exist | SWB-R36 |

## Phases

Each phase's exit is checked before the next starts. Every phase writes a
`docs/agent-notes/` entry and a dated TIN-4655 comment (R-N13).

*Re-sequenced 2026-09-25 (SWB-R28):* "The LGTM steps L0–L5 are interleaved
with the broker phases." The order is R0 → P1a → L0 → the SWB-R27 decision
→ P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4, and L5 is off that sequence: it
is independent of P4 and starts once its three gates hold (SWB-R36).
*Superseded 2026-09-25 (SWB-R36):* the order ended "… → L4 → P4 → L5".
The L phases are specified
in [ADR-0002](0002-lgtm-plane.md); this list keeps the P phases and names
the L phase between each pair. L0 comes after P1a in the sequence, and its
canary gate remains SWB-R29; P1b depends on P1a's exit, the recorded
SWB-R27 decision, and L0's usable read plane for the combined Junie/Pi
path (SWB-R41, SWB-R48). Collector scrubbing and telemetry provenance
proof follow v1 functional proof; they do not block the broker MVP.
Bodies remain off until the live ACL and redaction gates pass (SWB-R33,
SWB-R34). *Superseded 2026-09-26:* "nothing in P1b consumes L0's output".
Every drill
that stops, restarts or scales a process is performed by the operator
(R-N11); the agent asks and records the result.
*Superseded (P0):* the list read P0, P1a, P1b, P2, P3 ("Junie and LGTM"),
P4, with the scrape job, dashboard and alert drill bundled into P3.

- **P0: design and rulings.** Done 2026-09-25. Exit: the operator answered the
  P0 rulings, recorded as the two dated TIN-4655 comments above.
- **R0: LGTM rulings.** Done 2026-09-25. Exit: TIN-4655 comment `73f1ce72`
  records the answers, and this revision plus ADR-0002 carry them
  (SWB-R30). The repo note is
  `docs/agent-notes/2026-09-25-TIN-4655-r0-lgtm-adr-revision.md`.
- **P1a: substrate** (this repo and its fork; tailnet-acl only if needed,
  since the tags are reused).
  - Scope: repo scaffold, ruleset, merge queue, private-fork org setting,
    fork, and `fork-setup`.
  - Exit: a trivial PR from the fork runs through the merge queue green.
  - *Status 2026-09-25:* the exit test (PR #2) waits on runner-group
    admission, `xoxd-ai/tinyland-infra#106`, which the GF/infra lane
    sequences, not lab.
- **L0: read plane in lab.** After P1a in sequence; the canary (SWB-R29)
  and retargeting ACL proof (SWB-R42) gate its delivery. Scope and exit:
  ADR-0002. Usable reads are required for the P1b combined path (SWB-R41).
- **SWB-R27 decision.** Before P1b: the body-read ACL audit, as a dated
  TIN-4655 comment. Its scope is in ADR-0002 → Risks and L1.
  - *Done 2026-09-25:* TIN-4655 comment `f870729e` carries the TIN-4668
    answer "A: tailnet ACL, admins + MCP" as SWB-R27's decision. Bodies
    still wait for ACL A to be enforced live (SWB-R33); that gate is L1's,
    not P1b's.
  - *Superseded 2026-09-25:* "Its scope is in ADR-0002 → Risks and Open
    rulings."
- **P1b: broker MVP.** Repos: this one, blahaj (the
  `tofu/stacks/agent-switchboard` stack and a `tailnet-dns` alias) and lab.
  - The broker ships `register`, `peers`, `send`, `inbox` and `ack`, stamps
    `authority: peer`, validates envelope v3, serves `/metrics`, writes the
    audit stream to stdout after commit — without bodies until ACL A is
    enforced live and redaction is qualified (SWB-R19, SWB-R27, SWB-R33,
    SWB-R34) — and runs on SQLite on a
    PVC. The
    transactional outbox and the Tempo projector land in L2 (ADR-0002).
    - *Superseded (P0):* "writes the audit stream with bodies (SWB-R19)".
    - *Superseded 2026-09-25 (SWB-R33):* "without bodies until SWB-R27 is
      decided".
    - *Superseded (refutation, 2026-09-25):* "with the transactional
      outbox (ADR-0002)" in P1b; ADR-0002 scopes the outbox to L2.
  - Lab adds the `agents` entry to `vars/mcp_registry.yml` for claude_code,
    codex and opencode (Kimi inherits), with `codex_startup_safe` set
    deliberately. It then runs `export-registries.py` and the Codex inventory
    cassettes. The entry reaches Claude and Kimi through `tinyland.mcp` and
    `~/.claude.json` (Harness reach).
  - Lab adds the `SWB_HARNESS` export to `kimi-claude.sh`, and the
    SessionStart, UserPromptSubmit, Stop and SessionEnd hooks in
    `nix/home-manager/claude-code.nix`, merged at `mkDefault` through the
    existing `tinyland.claudeCode.hooks` `mkMerge`, with a contract test
    that the home-root, GUI-launch and local-build guard hooks are still
    present with the switchboard enabled (critique fix). In the same pass
    the rendered `settingsJson.telemetry.enabled` key is checked against
    upstream and removed if it is the inert key the research found, or the
    reason it stays is recorded ("remove false artifacts as you go").
  - Codex registers through `swb whoami` (Identity). No `notify` change.
  - Lab adds a new `peer-dialog` skill, with #1920 as its degraded transport
    and the ADR-0002 lookup recipes with their caveats.
  - Pi's `agents` profile lands here (SWB-R37) through lab's
    `pi_mcp_profile_policy`. Both Junie and Pi get explicit combined broker
    plus LGTM read access with measured tool budgets (SWB-R41, SWB-R48),
    preserving existing default groups/profiles. L0 is a functional
    prerequisite for that path; telemetry scrubbing and provenance proof
    are post-v1 qualification, with bodies kept disabled until qualified.
  - Lab contract tests use a named remote entrypoint: Honey primary,
    Sting fallback, with exact source and execution host in each receipt
    (SWB-R39). Neo remains the teletype seat.
  - Host enrollment follows Neo Claude ↔ Sting Codex acceptance with
    Honey/Bumble, then yoga/mbp-13; PZM stays behind storage delivery
    (SWB-R38, SWB-R45). Each host still needs delivery evidence.
  - Exit, all must hold:
    - Codex registration is evidenced: a Codex session on sting registers
      with an `agent_id` whose `pid` and `session_id` are its own, and the
      receipt records the binding Codex exposed (an environment variable
      or a per-session path), not pid ancestry (Identity). This sub-exit
      is named so the round trip below does not depend on it silently;
    - a Claude session on neo and a Codex session on sting complete a
      threaded `in_reply_to` round trip;
    - the inbox survives a pod restart, which the operator performs
      (R-N11);
    - with the broker down (the operator scales it to zero), sessions start
      normally and the skill falls back; and with the broker's address
      blackholed (packets dropped, not refused — a NetworkPolicy or host
      rule the operator applies) every hook still returns within 2 s and
      exits 0 (critique fix: a fast refusal does not prove the timeout);
    - the registry URL matches the live hostname, proven by a live probe
      receipt from a seat, because a hermetic test cannot see the tailnet
      (critique fix); the contract test pins the URL string only;
    - explicit Pi profile selection registers a session and completes a
      threaded round trip (SWB-R37);
    - both Junie and Pi complete broker and LGTM read calls in the same
      selected session, with measured combined tool budgets (SWB-R41).
- **L1: Loki bodies and audit** and **L2: Tempo dialog projection.** After
  P1b, in that order. ADR-0002.
- **P2: claims, handoff, push.** Repos: this one, blahaj (the Linear sops
  leaf) and lab (Home Manager for agentd on PZM, neo, honey, sting and
  bumble).
  - Claims ship with optional `exclusive` (SWB-R16).
  - Linear read and handoff-receipt comments ship (SWB-R15).
  - agentd includes neo (SWB-R17). Its telemetry is metadata only
    (SWB-R40, SWB-R47; Push).
  - Exit, all must hold:
    - two advisory claims on one TIN both succeed and each reports the
      overlap;
    - a second exclusive claim returns `held_by` and does not record;
    - a lease expires on its own;
    - a stale socket is detected and delivery falls back to pull;
    - a handoff posts exactly one receipt comment and moves no Linear state;
    - agentd on neo and PZM starts from its declared path (internal volume
      or proven `/nix` user-domain exec) and is still running after a
      reboot (critique fix);
    - neo's agentd steady state, measured over one hour with the long-poll
      and notice paths idle-cycling, stays within the proposed bound:
      resident set ≤ 32 MiB, CPU time ≤ 36 s per hour (1%), and ≤ 10
      wakeups per minute (critique fix: "recorded" alone had no
      threshold). The numbers are proposals with an owner (lab) and a
      deadline (the P2 plan comment, before the drill), confirmed or
      replaced there (ADR-0002 → Open rulings; SWB-R45 leaves numeric
      budgets proposed); PZM is measured against the same numbers.
- **L3: presence and graph metrics, dashboard, alert.** After P2. ADR-0002.
- **P3: Junie and census.**
  - Lab verifies the existing `ag` upstream and Junie reach delivered
    during P1b (SWB-R41), and adds the census textfile with a declared
    writable path per host (neo skipped).
  - *Superseded 2026-09-26 (SWB-R41):* the initial Junie broker enrollment
    was scheduled here. Its combined broker/LGTM functional path is P1b;
    P3 retains census and follow-on verification.
  - *Superseded (P0):* "tinyland.dev adds the scrape job. blahaj adds the
    dashboard and an alert." Those are L3 (SWB-R28).
  - Exit, all must hold:
    - provisioning shows `tracker` at 100 tools or fewer and every name at 64
      characters or fewer;
    - a Junie session completes a threaded round trip with an explicit `me`;
    - `swb_local_session` appears for PZM and one Linux host.
- **L4: Tempo owner-release upgrade.** After P3. ADR-0002.
- **P4: Codex push.** Only with a recorded live-thread proof (SWB-R20).
  Codex registration is P1b's (Identity), and no `notify` table is ever
  declared; the `notify` table the R0 plan had proposed is withdrawn.
  - Exit: on neo, with the computer-use `notify` present, the Codex
    converge writer changes nothing (critique fix); a live-thread proof
    receipt exists before any daemon push is enabled.
  - *Superseded (draft of this revision, 2026-09-25):* a P4 heading
    "Pi profile, Codex registration and push" that carried the Pi profile
    a second time; the Pi profile is now placed once, in P1b (SWB-R37).
- **L5: harness-native telemetry.** Independent of P4; it starts once its
  three gates hold — the TIN-4668 scrubbed endpoint, its scrubber and the
  enforced ACL (ACL A applied by TIN-4670) — whatever P phase is current
  (SWB-R31, SWB-R32, SWB-R36). Tempo carries tool names, file paths and
  ticket IDs as span attributes; bodies and full content stay in Loki
  (SWB-R35). ADR-0002.
  - *Superseded 2026-09-25 (SWB-R36):* "Gated on the scrubbed TIN-4668
    endpoint, its scrubber and the SWB-R27 decision (ruling 4; SWB-R31,
    SWB-R32). Whether it also waits for P4, as the adopted L0–L5 order
    implies, is ADR-0002 → Open rulings 3."

## Rulings

Quotes are the operator's words or picks as recorded in the cited source.
Operator questions and asides are not rulings (R-N13). The R0 LGTM-round
rulings, SWB-R25 to SWB-R32, and their amendment round, SWB-R33 to SWB-R36
(TIN-4655 `f870729e`), plus SWB-R37 to SWB-R48 (2026-09-26), are recorded in
[ADR-0002 → Rulings](0002-lgtm-plane.md#rulings) with the same discipline;
SWB-R22 to SWB-R24 are the P1a substrate rulings carried by the P1a
receipts (TIN-4655 comments `6274ecbd` and `fd195b08`) and their PRs.

| ID | Date | Source | Ruling |
| --- | --- | --- | --- |
| SWB-R01 | 2026-09-25 | Operator interview (lab seat); TIN-4655 description | Scope: "discovery, dialog, and advisory task claims/handoff." |
| SWB-R02 | 2026-09-25; reworded 2026-09-25 | Operator interview (lab seat); TIN-4655 description. Rewording: TIN-4655 comment `73f1ce72` (R0), answer "Re-word, don't reverse (Recommended)" | "Self-registration is authoritative; the broker writes through to LGTM, which is the read/query/context plane and never the commit path." Acks, claims, sequence and leases stay in the broker. *Superseded wording:* "self-registration is authoritative; LGTM is a view." |
| SWB-R03 | 2026-09-25 | Operator interview (lab seat); TIN-4655 description | Delivery: "mailbox plus native push where it exists; never tmux injection." |
| SWB-R04 | 2026-09-25 | same | Hosting: "a blahaj pod, tailnet-only." |
| SWB-R05 | 2026-09-25 | same | Broker: "Rust, with self-asserted identity." |
| SWB-R06 | 2026-09-25 | same | Tailnet: "reuse `tag:k8s,tag:mcp-proxy`." |
| SWB-R07 | 2026-09-25 | same | Delivery pace: "phased, design doc first." |
| SWB-R08 | 2026-09-25 | same | Repo: "`xoxd-ai/agent-switchboard`, forked as `Jesssullivan/agent-switchboard`." |
| SWB-R09 | 2026-09-25 | TIN-4655 comment `67611936` (P0 round one) | "Retention: acked 7 d, unacked 30 d; TTL 72 h (max 14 d)." *Addendum 2026-09-25 (SWB-R25):* broker retention only; Tempo and Loki keep 7 d. |
| SWB-R10 | 2026-09-25 | same | "Non-blocking hooks: 2 s timeout, always exit 0." |
| SWB-R11 | 2026-09-25 | same | "Registry targets claude_code, codex and opencode (Kimi inherits); VS Code is excluded." |
| SWB-R12 | 2026-09-25 | same | "Junie reaches the broker through the mcp-mux `tracker` group, never `mux`." |
| SWB-R13 | 2026-09-25 | same | "Allow private forks; fork PRs pass `ci-ok` as \"gated in merge queue\", with full CI on `merge_group`." |
| SWB-R14 | 2026-09-25 | TIN-4655 comment `643df6af` (P0 round two) | Authority: "Operator-directed flag with a ruling link". The broker still stamps `authority: peer`. |
| SWB-R15 | 2026-09-25 | same | Linear: "Read + comment". Read title, state and assignee; post handoff receipts as comments. Never move state or edit descriptions. |
| SWB-R16 | 2026-09-25 | same | Claims: "Optional exclusive, off by default". A second exclusive claim returns `held_by` and does not record. |
| SWB-R17 | 2026-09-25 | same | "Push adapter on neo too". Tiny: long-poll, at most one notice per 60 s, no bodies. (The former agentd-never-emits-OTLP proposal was not part of this ruling and was superseded by SWB-R40/SWB-R47 on 2026-09-26.) |
| SWB-R18 | 2026-09-25 | same | "Pi profile in v1". An `agents` Pi MCP profile, changing Pi's zero-MCP default for that profile only. |
| SWB-R19 | 2026-09-25 | same | "Message bodies in Loki". Retention and access follow Loki's. *Additions 2026-09-25 (SWB-R26, SWB-R27):* never Tempo attributes; stdout → Alloy with a `loki.process` stage; out of stdout until the body-read ACL is decided, before P1b. *Additions 2026-09-25 (SWB-R33, SWB-R34):* bodies ship only once ACL A is enforced live; the `loki.process` stage gets a redaction step with the TIN-4668 scrubber's pattern set. |
| SWB-R20 | 2026-09-25 | same | Unchanged: Codex push waits for a recorded live-thread proof. |
| SWB-R21 | 2026-09-25 | Operator interview, relayed to the P1a lane by the orchestrating session; durable carrier: the P1a receipt comment on TIN-4655 | "Yes, start P1a now." |
| SWB-R22 | 2026-09-25 | Operator interview, relayed by the orchestrating session after the pre-push hook refused a direct push to `main` (R-N12 stop) | "Yes, root commit via API, then PR". One GitHub-side root commit through the contents API is the only direct write to `main`. The scaffold then lands by PR before the ruleset is applied. |
| SWB-R23 | 2026-09-25 | same | "You flip it in the GitHub UI". The operator turns on forking of private repositories for xoxd-ai. Agents do not change org settings or switch gh logins. |

Estate rulings this design depends on:

- **R-N11, R-N12, R-N13** (TIN-3692; carried in `xoxd-ai/lab` `AGENTS.md`):
  - agents never signal processes;
  - a guard-hook refusal is a stop;
  - every mutating step cites a ruling ID in its receipt.
- **The 2026-08-31 no-containment ruling** (lab `AGENTS.md`, "Harnesses run
  unconstrained"). Advisory claims keep this broker inside it. The one narrow
  refusal, exclusive-vs-exclusive, carries its own ruling, SWB-R16.
- **The ban on state-writing Linear automation** (lab `AGENTS.md`, Tracker
  Hygiene, ratified 2026-09-03). SWB-R15 permits comments only.
- **GloriousFlywheel runners only** (lab `AGENTS.md`, 2026-09-06), and no
  hosted runners (site.scaffold `docs/CI-SCHEMA.md` §5).

## History

- **Unsigned root commit on `main`.**
  - `main`'s root commit is `89bfa39e0177128358a5a3931746382164471ac5`,
    "chore: initialize main". It was made through the GitHub contents API on
    2026-09-25, per SWB-R22, before the ruleset that requires signed
    commits existed. It was needed because a local pre-push hook refuses
    creating `main` directly.
  - GitHub reports it `unsigned`, because contents-API commits made with a
    personal token are not signed by GitHub.
  - Every commit after it is signed: the scaffold commit `2cff51d`, its
    GitHub-signed merge `2a11acb`, and every later commit, which the ruleset
    enforces.
  - **SWB-R24** (operator interview 2026-09-25): "Accept it, record why
    (Recommended)". The root commit stays as it is. Replacing it would need a
    force-push and a ruleset change, and neither is authorized.
