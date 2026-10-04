# ADR-0002: the LGTM plane

- **Status:** Accepted: its fork PR, agent-switchboard #4, merged
  (SWB-R30). Proposed 2026-09-25. The Opus refutation that SWB-R30
  requires ran on 2026-09-25 with the verdict "ship-with-fixes"; every
  problem it raised is handled in
  [Refutation review](#refutation-review-2026-09-25). Design choices below
  that no ruling names — the outbox plus the gauge, the numeric bounds, the
  reconciliation cadence, the keyed body hash — are proposals, not rulings.
  No L phase has started. *Amended 2026-09-25 (a second commit on the same
  PR):* the operator answered the first four Open rulings (TIN-4655 comment
  `f870729e`), recorded as SWB-R33 to SWB-R36 in [Rulings](#rulings); the
  remaining open items were renumbered 1–9. *Amended 2026-09-26:* those
  nine interview items are answered by SWB-R37–SWB-R45; SWB-R46–SWB-R48
  clarify duplicate conflicts, agentd metadata and the functional MVP
  boundary. Numeric budgets remain proposed, not ratified.
- **Date:** 2026-09-25
- **Linear:** TIN-4655 (R0 comment `73f1ce72-28c5-42d6-9039-1135e1c92121`,
  2026-09-25T17:57Z; premise correction `26a311db`, 18:19Z; answers to Open
  rulings 1–4 `f870729e-dbcb-451d-97c3-0c1e1f8e9641`, 19:11Z); TIN-4668 (the
  tailnet OTLP ingest request; comments `875fc21f` step-0 evidence and
  probes, `1001c0fb` repo, read-ACL and open-path rulings, `a9701697` the
  tinyland.dev-PR answer, `605d0cc7` the ACL repo); TIN-4670 (closing the
  retained open tailnet path; created 2026-09-25T18:29Z, `Backlog` per its
  `stateHistory`); TIN-75 (host OTLP convergence; `In Review` since
  2026-08-21T16:50Z per its `stateHistory`, not read from the status
  field).
- **Amends:** [ADR-0001](0001-agent-switchboard.md) — SWB-R02 (reworded),
  the "LGTM" section, the P3 bundling, the dashboards-and-alerts line, the
  Codex identity line, and the Push section (the former agentd OTLP ban,
  superseded by SWB-R40/SWB-R47 on 2026-09-26).
- **Sources:**
  - the operator's direction, verbatim (TIN-4655 comment `f5f30195`):
    "similarly, pleas workflow oon the agent-switchboard regarding rollout,
    enrolment, HM integration, as well as plan deep LGTM integration (tempo
    is a great canidate for chat dialogs) such that LGTM lookups can be a
    centeral agent, dialog, context and peer discover and chat substrate.";
  - the read-only planning workflow `wf_6c93c676-1a3` (five research lanes,
    a Fable synthesis, an Opus 5.5 critique with the verdict
    "sound-with-fixes"), kept in the lab seat's rescue folder as
    `agent-switchboard-lgtm-plan-wf_6c93c676-1a3.json`;
  - the R0 rulings, comment `73f1ce72`, quoted in [Rulings](#rulings);
  - the Opus refutation of this ADR's first draft (2026-09-25, verdict
    "ship-with-fixes", 23 problems and 7 ruling-fidelity findings), handled
    in [Refutation review](#refutation-review-2026-09-25);
  - the operator's later 2026-09-25 statements on TIN-4655 (`26a311db`) and
    TIN-4668 (`1001c0fb`, `a9701697`, `605d0cc7`), quoted where cited;
  - the operator's answers to this ADR's first four Open rulings, TIN-4655
    comment `f870729e` (2026-09-25T19:11Z, "Open rulings on ADR-0002 /
    agent-switchboard #4"), quoted in [Rulings](#rulings) as SWB-R33 to
    SWB-R36.
- **Pinned evidence.** Estate facts below cite a file and line range at a
  pinned commit, or a tool call made by a research lane on 2026-09-25:
  - `tinyland.dev@68a16f65` = `xoxd-ai/tinyland.dev` `main` at
    2026-09-25T00:52Z (`68a16f652223d85c3b93ac2cdc01167749e3a3b6`), still
    the `main` tip at 2026-09-25T19:00Z (`gh api`). The critique found the
    plan's `94535f35` citations 67 commits stale; every tinyland.dev line
    below was re-read at `68a16f65` on 2026-09-25;
  - `blahaj@2071613c` = `xoxd-ai/blahaj` `main` at 2026-09-25T16:55Z.
    `main` moved to `917e952b` at 18:18Z; the one blahaj file this ADR
    quotes by line, `deploy/honey/retained-observability-tailnet-services.yaml`,
    has the same blob (`f56601ab`) at both, so its line numbers hold;
  - `lab@3d755193` = `xoxd-ai/lab` `main` at 2026-09-25T17:10Z;
  - `tailnet-acl@84ba9829` = `Jesssullivan/tailnet-acl` `main` at
    2026-09-19T16:24Z (the public Dhall repo the operator named as the ACL
    source: "actl is in public dhall acl repo", TIN-4668 comment `605d0cc7`);
  - Tempo documentation at tags `v2.7.2` and `v3.0.3`, and Tempo `main` at
    `da9051bd` (2026-09-25T17:50Z) for the two files the lanes read on
    `main`; OTel specification `main` at `cac5b81e` (2026-09-23);
    Prometheus `main` at `270db291` (2026-09-25); Loki `main` at `98555bd7`
    (2026-09-25); the cited lines were re-read at those shas on 2026-09-25;
    mcp-grafana at `v1.6.0` (released 2026-09-25T10:31Z).

## Context

### What the operator asked for

A plane where "LGTM lookups can be a central agent, dialog, context and
peer discover and chat substrate", with Tempo carrying chat dialogs. R0
settled how far that goes: the broker stays the system of record and writes
through; LGTM is the read, query and context plane and never the commit
path (SWB-R02, reworded).

### What runs today

- **Tempo 2.7.2**, one replica, `Recreate`, on sting; PVC `tempo-store` 5 Gi
  on `local-path-sting`; blocks in the RustFS bucket
  `tinyland-dev-observability`; `block_retention: 168h`; the metrics
  generator has only storage paths, so there are no span metrics and no
  service graph (`tinyland.dev@68a16f65 infra/staging/monitoring.yaml:776,
  893-937`; tool call `grafana_api_request GET
  /api/datasources/proxy/uid/tempo/api/status/buildinfo` → v2.7.2;
  `list_prometheus_metric_names` regex `^traces_.*` → none).
- **The collector's OTLP receivers are on the tailnet today, unscrubbed.**
  The collector is owned by `xoxd-ai/tinyland.dev`
  (`infra/staging/otel-collector.yaml`), not blahaj (TIN-4655 comment
  `26a311db`); blahaj owns only the retained tailnet Services in front of
  it. In-cluster they are `otel-collector.tinyland-staging.svc:4317/4318`; on
  the tailnet blahaj's retained Services expose the same collector as
  `otlp-observability-grpc:4317` and `otlp-observability-http:4318`
  (`blahaj@2071613c deploy/honey/retained-observability-tailnet-services.yaml:58-102`),
  beside Tempo's query port `tempo-observability:3200` (`:104-125`), and
  the registry lists both OTLP names as tailnet ingest, "traces + metrics"
  (`tinyland.dev@68a16f65 docs/monitoring/observability-endpoint-registry.md:279-282`).
  Those Services feed the in-cluster pipelines that have no redaction and
  that upsert `k8s.cluster.name` and `service.namespace`; GET probes from
  neo on 2026-09-25 got 405 from `/v1/traces` and `/v1/metrics` (the
  handlers exist), 404 from `/v1/logs` (no logs pipeline), and 200 with no
  credentials from Loki `/loki/api/v1/labels` and Tempo `/api/search/tags`
  (TIN-4668 comment `875fc21f`). The R0 comment's premise that "the OTel
  collector is in-cluster only, and the tailnet exposes only Tempo's query
  port 3200" is out of date; the operator's dated correction is TIN-4655
  comment `26a311db`. The unscrubbed tailnet 4318 (and 4317) is ticketed as
  TIN-4670, which closes it (operator: "Ticket it, fix with the build",
  TIN-4668 comment `1001c0fb`). The registry rule is
  unchanged: the collector is the only ingest authority, "Do not send OTLP
  directly to Tempo" (`registry.md:350-352`).
- **Tempo limits** at 2.7.2 defaults, not overridden in staging:
  `max_span_attr_byte` 2048, any longer attribute truncated;
  `max_bytes_per_trace` 5 MB; `max_traces_per_user` 10,000; search
  `default_result_limit` 20 and `max_duration` 168h; TraceQL metrics
  `max_duration` 3h; streaming off; no `/api/mcp` (Tempo
  `docs/sources/tempo/configuration/_index.md:240,1830` at `v2.7.2`;
  `monitoring.yaml:893-900` sets only receivers; tool calls
  `grafana_api_request GET …/tempo/status/config` and `GET …/tempo/api/mcp`
  → 404, tempo-dialog and estate-lgtm lanes).
- Tempo can lose up to about 45 minutes of traces on an unclean exit:
  `max_block_duration: 30m` plus `complete_block_timeout: 15m`
  (`monitoring.yaml:921-929`; `tinyland.dev@68a16f65
  docs/plans/lgtm-store-durability-2026-08-24.md:461-463`).
- Tempo search returns the first N matches, not the newest, and
  `with(most_recent=true)` is non-deterministic; measured on the live
  instance, a limited query over a two-minute window returned the oldest
  traces in it (tempo-dialog lane, `GET …/tempo/api/search?q={}
  with(most_recent=true)&limit=50`). Spans were searchable about 8 s after
  their start (`ingester.trace_idle_period: 10s`).
- **Loki 3.6.3**, one replica on sting, `auth_enabled: false`,
  `retention_period: 168h`, `max_line_size` 256 KB, structured metadata on
  (`monitoring.yaml:512,626,721`; Loki `docs/sources/shared/configuration.md`
  defaults, tempo-dialog lane). Ingest is Alloy tailing pod stdout from an
  explicit allowlist of ten namespaces, the Faro receiver, and the tailnet
  push `loki-observability:3100`, which is "a push target **only** for the
  host journald-to-Loki shipper … Applications do not push to Loki: they
  write to stdout and Alloy collects" (`infra/staging/alloy.yaml:326-345`;
  `registry.md:284-293`). The OTel collector has no logs pipeline
  (`infra/staging/otel-collector.yaml:56-64`).
- The Alloy `pod_logs` pipeline parses only `level`, `message`, `service`
  and `component` and labels `level` and `service`; there is no
  `structured_metadata` stage and no `op` label (`alloy.yaml:385-410`).
- **Mimir 2.17.10**, one replica on sting, `multitenancy_enabled: false`,
  `max_global_series_per_user: 1000000`, blocks kept 168h
  (`infra/staging/mimir.yaml:205,347,351`; version from the estate-lgtm
  lane's `query_prometheus` of `cortex_build_info{job="mimir"}`). Two writers feed it, the staging
  Prometheus and Alloy, under the rule that "a target belongs to exactly
  one of these two" (`registry.md:407-425`).
- **prometheus-mail** (prom/prometheus v2.51.0 with alertmanager v0.27.0,
  namespace `tinyland-dev-production` on honey; estate-lgtm lane,
  `query_prometheus` of `kube_pod_container_info`; deployed by
  `blahaj@2071613c tofu/stacks/mail/modules/monitoring/main.tf:204-242`)
  plus its Alertmanager is the only paging surface. It scrapes its own
  static jobs and has no remote read from Mimir; the precedent for seeing
  staging series is passive federation
  (`blahaj@2071613c tofu/stacks/mail/modules/monitoring/templates/network-federation.yml.tftpl:1-22`,
  TIN-4324). lab's `AGENTS.md` names prometheus-mail rules plus the mail
  Alertmanager as the estate alerting surface, not Grafana unified
  alerting (2026-08-29 ratification).
- **Grafana 12.3.6** (`12.3.6+security-04` from the estate-lgtm lane's
  `GET /api/health`; pod on bumble per `kube_pod_info`; Grafana Live off,
  `GF_LIVE_MAX_CONNECTIONS` `"0"` in `monitoring.yaml`'s Grafana
  environment at `68a16f65`) provisions five read-only datasources. The
  Tempo datasource already links traces to Loki by trace id
  (`tracesToLogs.filterByTraceID: true`), to Prometheus for metrics and the
  service map, and has `lokiSearch` (`monitoring.yaml:1862-1879`; tool call
  `get_datasource(uid=tempo)`). Dashboards are file-provisioned from
  ConfigMaps, because the MCP service account has no `dashboards:create`
  (`registry.md:332`).
- **The Grafana MCP** is mcp-grafana **0.17.2** (`lab@3d755193
  nix/packages/mcp-grafana.nix:19`), systemd user units on honey bound to
  `127.0.0.1` and published by `tailscale serve --http <port>
  http://127.0.0.1:<port>` (`lab@3d755193 nix/modules/grafana-proxy.nix:90,120`),
  reached at `http://honey.taila4c78d.ts.net:8091/mcp`, authenticating as
  the Viewer service account `sa-1-tinyland-mcp-grafana`, bound to the
  legacy L4 Grafana IP `100.74.127.80:3000` (`registry.md:586-600`). It has
  no Tempo tools: 0.17.2 proxies only Tempo's own `/api/mcp`, which 2.7.2
  lacks. mcp-grafana 1.5.0 added native read-only Tempo tools that call the
  Tempo HTTP API directly (mcp-grafana `CHANGELOG.md:43` at `v1.6.0`, PR
  #1194). During the synthesis session the grafana-tailnet MCP timed out
  three times against `100.74.127.80:3000` (tool calls
  `list_prometheus_metric_names` and two `query_prometheus`, "net/http:
  timeout awaiting response headers").
- **The OTel collector** (contrib 0.118.0, one replica on honey; owned by
  `xoxd-ai/tinyland.dev`, `infra/staging/otel-collector.yaml`) receives
  OTLP gRPC 4317 and HTTP 4318, upserts `k8s.cluster.name=honey` and
  `service.namespace=tinyland-staging` on every span and metric, and has
  traces and metrics pipelines only; its queue is in memory
  (`otel-collector.yaml:13-19,29-36,56-64`). `tinyland-staging` denies all
  ingress by default; 4317/4318 are admitted from the same namespace,
  Tailscale-managed proxies and `financebro`
  (`infra/staging/network-policy.yaml:4-11,48-67,91-99`).
- **Auth.** The tailnet OTLP, Loki and Tempo endpoints have no client
  auth: "No credentials on any of these. Tailnet membership is the auth"
  (`registry.md:293`), confirmed by the 2026-09-25 probes above. The ACL
  grants `group:dollhouse-admins` and `group:dollhouse-users` access to
  `tag:k8s:*` and to `tag:k8s-operator:*`
  (`tailnet-acl@84ba9829 fragments/kubernetes.dhall:39-46`); honey, bumble
  and sting are granted `tinyland-loki-observability:3100` and honey alone
  `tinyland-grafana-observability:3000` (`kubernetes.dhall:60-67`). Every
  Grafana user can read every datasource, because OSS Grafana has no
  per-datasource permissions, and so can the Grafana MCP proxy
  (`875fc21f`). Only sting holds `tag:k8s`; honey and bumble were refused
  at an operator-proxy port (`blahaj@2071613c
  docs/reference/observability-stack-topology.md:95-100`; lane confidence
  medium). **The operator has answered the read-ACL question** for the
  shared body-read decision (TIN-4668 item 4 is "the same body-read ACL
  decision as agent-switchboard message bodies (TIN-4655 R0 ruling 2)"):
  "A: tailnet ACL, admins + MCP" — raw Loki 3100 and Tempo 3200 limited to
  `group:dollhouse-admins` and `tag:mcp-proxy`, Grafana as the single human
  surface with its user list audited (TIN-4668 comment `1001c0fb`, tracked
  on TIN-4670; the change lands in the public Dhall ACL repo
  `Jesssullivan/tailnet-acl`: "actl is in public dhall acl repo",
  `605d0cc7`). It is decided, not applied: nothing has changed live, and
  TIN-4670 is `Backlog`. *Added 2026-09-25 (SWB-R33, `f870729e`):* that
  answer is SWB-R27's decision, and `swb` bodies ship only after TIN-4670
  applies ACL A in `Jesssullivan/tailnet-acl` and a raw Loki read from a
  non-admin tailnet node is refused.
- **The contract.** blahaj admits one LGTM release, owned by tinyland.dev,
  `acceptance_state: migration_blocked`, `retention_days` 7 for logs,
  metrics, profiles and traces, and forbids an `alternate-lgtm-mcp` and a
  `duplicate-or-undeclared-collector` (`blahaj@2071613c
  config/observability-authority.json:17,20,83-87,108-112`). The registry
  adds: "Anything needed beyond seven days must be captured as a Grafana
  snapshot, a dashboard annotation, or a written receipt"
  (`registry.md:576-578`).
- **Harness telemetry** is off fleet-wide: `tinyland.claudeCode.telemetry.enable`
  defaults to false and nothing sets `CLAUDE_CODE_ENABLE_TELEMETRY`; the
  rendered `settingsJson.telemetry.enabled` key (`lab@3d755193
  nix/home-manager/claude-code.nix:187-188`) has no documented upstream
  settings key (harness-telemetry lane, `code.claude.com/docs/en/settings-reference`),
  and `telemetry` is not in the activation's `managed_keys`
  (`claude-code.nix:813`). No other harness has any OTel rendering. The
  only lab-host OTLP producer is softconnect on PZM (opentelemetry 0.32,
  http/protobuf, fixed endpoint, bounded queue, 1 s connect and 2 s total
  timeout, no retry or spool; `Jesssullivan/softconnect@5b285948
  src/native_tracing.rs:23-31`). Its spans reach Tempo relabelled as
  `honey`/`tinyland-staging`, which is the live evidence that the retained
  tailnet OTLP path is in use today (`875fc21f`).
- **TIN-4668 in flight.** A PR in `xoxd-ai/tinyland.dev`, the collector's
  owner, on branch `feat/tin-4668-otlp-tailnet-ingest` is being built by
  the blahaj seat (operator:
  "tinyland.dev PR (Recommended)", `1001c0fb`; "tinyland.dev PR, and flag
  the open 4318 (Recommended)", `a9701697`): a separate `otlp/tailnet`
  HTTP receiver on 14318, pipelines `traces/tailnet` and `logs/tailnet`,
  redaction and scrub processors, no resource upsert on the new pipelines,
  logs to Loki's native OTLP endpoint, and a new tailnet Service
  `otlp-harness-http-tailscale` with port 4318 → target 14318 (`875fc21f`).
  It "also enforces TIN-4655 R0 ruling 2 (no bodies in Tempo span
  attributes) on `traces/tailnet`" (`1001c0fb`), which means harness
  content on that path lands in Loki, not Tempo; SWB-R35 (2026-09-25) now
  rules that scope for harness telemetry. Nothing is deployed, and
  it merges only on the operator's word.

### Why LGTM cannot be the system of record

- Tempo's API is ingest and query only. It cannot delete, update, ack,
  compare-and-set, subscribe or tail; 3.0 adds only admin redaction
  (`tempo-cli redact`) (Tempo `docs/sources/tempo/api_docs/_index.md:25-57`
  and `release-notes/v3-0.md:171-178` at `main` `da9051bd`). Streaming
  search returns partial results of one query and ends
  (`api_docs/_index.md:1021-1049`, same pin).
- Loki's tail is best-effort, closes after 1 h and allows 10 concurrent
  tails (Loki `docs/sources/reference/loki-http-api.md:1163-1180` at `main`
  `98555bd7`).
- Prometheus drops a series from instant queries 5 minutes after its last
  sample, so it cannot hold a lease (Prometheus
  `docs/querying/basics.md:505-529`, "Staleness", at `main` `270db291`).
- Retention: 7 days everywhere in LGTM against 30 days unacked and a
  14-day TTL in the broker (SWB-R09, SWB-R25).
- OTel exports a span only after it ends and it is immutable afterwards
  (OTel specification `specification/trace/sdk.md:1017-1027`, `OnEnd`, at
  `main` `cac5b81e`); Tempo splits a long-running trace across blocks, and
  structural TraceQL evaluates only within a block (Tempo docs,
  `troubleshooting/querying/long-running-traces/`, read by the tempo-dialog
  lane on 2026-09-25). A thread or a session therefore cannot be one trace.
- Duplicates: Tempo dedupes spans by kind and span id only when it assembles
  a trace by id; search and tag values can count a resend twice
  (`pkg/model/trace/combine.go:19-24,101-109` at `main` `da9051bd`).

## Decision

### Principles

1. **The broker is the system of record** for delivery states, `seq`,
   idempotency, acks, expiry, claims, leases, handoff receipts and the
   long-poll that agentd rides. Truth is read from the broker's own tools:
   `peers`, `inbox`, `thread`, `claims`.
2. **The broker writes through to LGTM after commit**, never before, never
   instead. LGTM is never in the commit path (SWB-R02).
3. **Broker and agentd telemetry have distinct custody** (SWB-R40,
   SWB-R44, SWB-R47; 2026-09-26). The broker alone projects its committed
   coordination state. Agentd may emit `swb` lifecycle metadata, counts and
   errors, never bodies or other content; its bounded event shape remains
   design work. The collector must prove source identity before treating
   any line, span or series as broker or agentd output, rejecting spoofed
   client-supplied identity; enrichment alone is insufficient. Broker tools
   remain authoritative for consequential state. This telemetry proof is
   post-v1 qualification, not a broker MVP gate (SWB-R48). Host harnesses
   emit their own telemetry under L5 (SWB-R31).
4. **LGTM is the read, query and context plane**: history, threads by
   message, peers as corroboration, graphs and dashboards, and (L5)
   harness-native telemetry. For harness telemetry Tempo is the recency
   index: tool calls, referenced files and paths and recently accessed
   tickets as span attributes, so an agent orienting to a workstream does
   not re-mine Loki; bodies and full content stay in Loki (SWB-R35).
5. **Hooks never touch LGTM.** The 2 s / exit 0 rule (SWB-R10) is unchanged,
   and the projection cannot delay a send.
6. **Bodies live in Loki only** (SWB-R19, SWB-R26), gated by the body-read
   ACL decision (SWB-R27) and shipped only once that decision, ACL A, is
   enforced live (SWB-R33); they pass a redaction step in the broker
   namespace's `loki.process` stage (SWB-R34). Tempo carries hashes and
   sizes.
7. **A reader of LGTM sorts by `swb.seq`** and treats absence as "expired
   from the view (7 d) or not projected", never as "never sent".

### Architecture by component

| Component | Role | Owner (change lands in) | Sequencer |
| --- | --- | --- | --- |
| `swb` broker (blahaj pod, Tailscale Service `mcp-agents`) | System of record; the only OTLP and stdout-audit writer; outbox and reconciliation | `xoxd-ai/agent-switchboard` (code), `xoxd-ai/blahaj` (pod, tofu stack, `tailnet-dns`, Linear leaf) | blahaj |
| Tempo | Dialog skeleton: one short trace per message and per lifecycle event, claims and sessions; hashes, never bodies. L5: the harness recency index — tool names, file paths and ticket IDs as span attributes, never bodies (SWB-R35) | `xoxd-ai/tinyland.dev` owner release (NetworkPolicy grant, L4 upgrade) | blahaj |
| Loki | Bodies and audit lines from broker stdout via Alloy, redacted in `loki.process` (SWB-R34); L5 harness content through `logs/tailnet` | tinyland.dev owner release (Alloy allowlist, `loki.process` stage with its redaction step) | blahaj |
| Mimir / staging Prometheus | Presence gauges, edge counters, projection health, one static scrape job | tinyland.dev owner release | blahaj |
| prometheus-mail | Paging alerts, fed by a federation match | `xoxd-ai/blahaj` | blahaj |
| Grafana dashboards | File-provisioned `swb` dashboard | tinyland.dev owner release (ConfigMap) | blahaj |
| Grafana MCP (`grafana-tailnet`) | The read surface for Claude Code, Codex, OpenCode, Kimi; Junie only in the `ops` group | `xoxd-ai/lab` (`nix/packages/mcp-grafana.nix`, `nix/modules/grafana-proxy.nix`) | lab |
| Enrollment, hooks, skill, agentd, mcp-mux, Pi profile | Home Manager delivery | `xoxd-ai/lab` | lab |
| Tailnet grants (read ACL A on Loki 3100 and Tempo 3200, honey to canonical Grafana) | ACL | `Jesssullivan/tailnet-acl` (public Dhall repo, `605d0cc7`; TIN-4670) | operator |
| Tailnet OTLP/HTTP ingest with scrubbing and a logs pipeline (L5): Service `otlp-harness-http-tailscale`, port 4318 → target 14318, receiver `otlp/tailnet` | Harness telemetry ingest | tinyland.dev owner release (the collector is tinyland.dev's), requested as TIN-4668 (branch `feat/tin-4668-otlp-tailnet-ingest`, being built by the blahaj seat) | blahaj |
| Retiring or repointing the retained `otlp-observability-{grpc,http}` 4317/4318 Services | Closing the unscrubbed path | `xoxd-ai/blahaj`, TIN-4670, after the scrubbed endpoint deploys; retire or repoint is the operator's pick at that point | blahaj |

lab and this repo request tinyland.dev changes by ticket and never add a
switchboard-side collector or a second LGTM MCP (`observability-authority.json:108-112`).

**Ingest path.** Broker → `otel-collector.tinyland-staging.svc:4318`,
OTLP http/protobuf, following the softconnect exporter shape (bounded
queue, batches of at most 32 spans and 64 KiB, 1 s connect and 2 s total
timeout) with one deliberate difference: retries are driven from the outbox
below, because the outbox is the spool and lives in the broker's durable
store. Never direct to Tempo (registry rule); never from a host and never
from agentd, by design (principle 3) and for neo's budget — not because
hosts cannot reach an OTLP proxy: they can today, through the retained
unscrubbed path that TIN-4670 closes (Context). The broker namespace needs
a NetworkPolicy grant to the collector's 4317/4318, and a second grant to
Tempo 3200 and Loki 3100 for the reconciliation reads below
(`network-policy.yaml:4-11,48-67` is default-deny and admits only the
listed sources today).

### Data model

#### Identifiers, derived by the broker

`msg_id` is client-suppliable (`schemas/envelope-v3.schema.json`
`properties.msg_id`), so the broker, not the client, derives every trace and
span id (critique fix): a client cannot pick an arbitrary trace id in the
shared single-tenant Tempo, and redeliveries do not collapse. The
protection is only as strong as `msg_id` uniqueness, and neither ADR-0001
(Mailbox) nor the schema scoped it, so SWB-R46 now fixes the scope (2026-09-26):
**`msg_id` is globally unique at the broker.** The message table keys on
`msg_id` alone; a `send` that reuses a stored `msg_id` with the same `from`
and the same body hash is the idempotent retry ADR-0001 allows and returns
the stored receipt; a `send` that reuses a stored `msg_id` with a different
`from` or a different body returns an opaque conflict without the stored
receipt or any other stored-message metadata, and stores nothing. Exact
retries remain idempotent; no second message or span lands in that trace.
ADR-0001 → Mailbox carries the same statement.

| Object | `trace_id` (16 bytes) | root `span_id` (8 bytes) |
| --- | --- | --- |
| message send | `sha256("swb:v1:msg:" ‖ msg_id)[0..16]` | `sha256("swb:v1:span:send:" ‖ msg_id)[0..8]` |
| lifecycle event `step` ∈ {notify, fetch, ack, expire} | `sha256("swb:v1:" ‖ step ‖ ":" ‖ msg_id ‖ ":" ‖ delivery_count)[0..16]` | `sha256("swb:v1:span:" ‖ step ‖ ":" ‖ msg_id ‖ ":" ‖ delivery_count)[0..8]` |
| claim event `step` ∈ {claim, release, handoff} | `sha256("swb:v1:" ‖ step ‖ ":" ‖ claim_id)[0..16]` | `sha256("swb:v1:span:" ‖ step ‖ ":" ‖ claim_id)[0..8]` |
| session event `step` ∈ {register, ended} | `sha256("swb:v1:" ‖ step ‖ ":" ‖ agent_id ‖ ":" ‖ proc_start)[0..16]` | `sha256("swb:v1:span:" ‖ step ‖ ":" ‖ agent_id ‖ ":" ‖ proc_start)[0..8]` |

`delivery_count` is 0 for `ack` and `expire`, and the count of the delivery
for `notify` and `fetch`, so each at-least-once redelivery is its own span
(the critique's point against `sha256(msg_id ‖ step)`). Because the send
trace id is a pure function of `msg_id`, a reader that can hash still has a
direct `get_tempo_trace` lookup; a reader that cannot searches on
`span.swb.msg_id`. An all-zero result is re-hashed with a `:1` suffix
(OTel forbids zero ids).

**One trace per event.** A message is one short trace whose root span is
`swb.send`. Each lifecycle event is its own short trace whose root span
carries a span **link** to the send span (`trace_id`, `span_id` of the
message). A reply's `swb.send` span links to the send span of the message it
replies to. Nothing is ever a late child of an earlier trace, and no span is
held open (ruled: "Each lifecycle step gets its own trace, linked back to
the send span"; the plan's shared-`trace_id` alternative is withdrawn
because it recreates the long-running trace).

**Timestamps** are the broker's commit time (one clock), 1–2 ms spans, so
skew between neo, PZM and the Linux hosts cannot reorder a thread.

#### Tempo spans

Resource attributes on every span: `service.name = "swb-broker"`,
`service.version = <image digest, short>`, `host.name = <pod>`. The
collector will upsert `k8s.cluster.name=honey` and
`service.namespace=tinyland-staging` (`otel-collector.yaml:29-36`); readers
separate switchboard data by `resource.service.name`, never by namespace or
tenant.

`swb.send` (kind PRODUCER):

| Attribute | Value |
| --- | --- |
| `swb.msg_id`, `swb.thread_id` | ULIDs |
| `swb.seq` | int, broker-assigned |
| `swb.from`, `swb.to` | `agent_id` |
| `swb.from_harness`, `swb.from_host`, `swb.to_harness`, `swb.to_host` | split for cheap filters |
| `swb.ticket` | `^TIN-\d+$` or `none` |
| `swb.authority` | constant `peer` (SWB-R14) |
| `swb.operator_directed`, `swb.ruling` | the sender's claim and its pointer |
| `swb.transport` | `broker`, `direct`, `ssh-forward`, `linear` |
| `swb.in_reply_to` | ULID or empty |
| `swb.body_size`, `swb.body_hmac`, `swb.artifact_refs` | body never present (SWB-R26). `body_hmac` is HMAC-SHA256 over the body under a broker-held key (a blahaj-side sops leaf), not a bare `sha256`: Tempo is readable without credentials today, and a bare hash lets any reader confirm a guessed short body such as "yes" or "done" (Risks) |
| `gen_ai.conversation.id`, `gen_ai.agent.id` | `thread_id`, `from` (GenAI conventions, Development status) |
| `messaging.system`, `messaging.operation.type`, `messaging.message.id` | `swb`, `send`, `msg_id` |

Lifecycle spans: `swb.notify` and `swb.fetch` (messaging `receive`,
attribute `swb.delivery_count`), `swb.ack` (`settle`, kind CLIENT),
`swb.expire`; each carries `swb.msg_id`, `swb.thread_id`, `swb.seq` and
`swb.to`, plus the link. Claim spans: `swb.claim` (`swb.subject`,
`swb.exclusive`, `swb.held_by`, `swb.overlaps_count`, `swb.lease_until`),
`swb.release`, `swb.handoff` (links the source claim; `swb.receipt_url`
once the Linear comment posts). Session spans: `swb.register` and
`swb.ended` (`swb.agent_id`, `swb.harness`, `swb.host`, `swb.pid`,
`swb.proc_start`). Heartbeats are not spans; presence lives in gauges.

Optional harness link (L5): when `swb send` runs from a Claude Code Bash
tool with enhanced telemetry on, the CLI reads `TRACEPARENT` and the send
span links to the harness tool span. The broker never parents harness spans.

#### Harness span attributes (L5, SWB-R35)

Ruled 2026-09-25 (SWB-R35, TIN-4655 comment `f870729e`): for harness-native
telemetry, Tempo is the recency index — "recency == relacence", in the
operator's words — so an agent orienting to a workstream, sprint, epic or
codebase asks Tempo what was touched recently instead of re-mining Loki.
Harness spans on `traces/tailnet` therefore carry, as span attributes:

| Carried | Never carried |
| --- | --- |
| the tool name of each tool call | prompts |
| the file paths and other paths a tool call referenced | tool input and tool output |
| the ticket IDs (`TIN-####`) a session accessed | assistant responses and raw API bodies |

- Bodies and full content stay in Loki, through `logs/tailnet` (SWB-R31's
  "Full content, scrubbed at the collector" is unchanged; only its store is
  Loki). This also fits Tempo's 2048-byte attribute cap
  (`configuration/_index.md:240` at `v2.7.2`): a path or an ID fits, a
  body does not.
- The exact attribute keys, and whether a harness emits a path or ticket ID
  natively or the collector derives it from an event, are fixed in the L5
  plan comment against what each harness actually emits and what the
  TIN-4668 `traces/tailnet` key allowlist admits. That is a proposal; the
  ruled part is the content split above. A derived attribute holds the path
  or the ID, never the input it was derived from.
- The scrubber runs on these attributes too (L5 exit): a path can itself
  contain a secret-shaped string.

Dedicated Parquet columns (L4, tinyland.dev): `swb.thread_id`,
`swb.msg_id`, `swb.from`, `swb.to`, `swb.ticket`, `swb.subject` as string
columns. vParquet4 allows up to 10 string columns per scope and no integer
columns; vParquet5 allows 20 strings and 5 integers, so an integer column
for `swb.seq` needs vParquet5 (Tempo
`docs/sources/tempo/operations/dedicated_columns.md:68-69` at `v3.0.3`).

#### Loki line schema

One JSON line per mutation on broker stdout:

```json
{"ts":"2026-09-25T18:00:00.123Z","op":"send","me":"claude:neo:4242:s1",
 "from":"claude:neo:4242:s1","to":"codex:sting:9001:t7","ticket":"TIN-4655",
 "ruling":"","msg_id":"01K5…","thread_id":"01K5…","seq":3,"in_reply_to":"",
 "delivery_count":0,"size":812,"body_hmac":"…","trace_id":"<32 hex>",
 "span_id":"<16 hex>","body":"…"}
```

- `op` ∈ {`send`, `notify`, `fetch`, `ack`, `expire`, `register`, `ended`,
  `claim`, `release`, `handoff`, `reconcile`}.
- `body` is present only on `send`, only once ACL A is enforced live
  (SWB-R33: TIN-4670 has applied it in `Jesssullivan/tailnet-acl` and a raw
  Loki read from a non-admin tailnet node is refused), at most 16 KiB (Loki
  `max_line_size` 256 KB). Until then `size` and `body_hmac` (the same
  keyed hash as the span attribute) stand in for it. The broker does not
  redact; senders never put a secret in a body (SWB-R19). These lines go
  stdout → Alloy → Loki and never through the TIN-4668 collector redaction
  processor, so the redaction happens in Alloy instead: the broker
  namespace's `loki.process` stage gets a redaction step with the same
  pattern set as the TIN-4668 collector scrubber (SWB-R34).
- **Stream labels** (low cardinality only): `service_name="swb"`, `op`,
  plus the pod labels the existing `discovery.relabel "pod_logs"` block sets.
  **Structured metadata**: `msg_id`, `thread_id`, `trace_id`, `span_id`,
  `from`, `to`, `ticket`. High-cardinality keys never become labels (Loki
  labels guidance, `docs/sources/get-started/labels/_index.md:119-123`).
- **Route:** broker stdout → Alloy `loki.source.kubernetes` → a
  `loki.process` block for the broker namespace (SWB-R26). The existing
  `pod_logs` stage would drop every id key and yield no `op` label
  (`alloy.yaml:385-410`), so L1 adds, in tinyland.dev, illustratively:

  ```river
  loki.process "swb" {
    // SWB-R34: redaction first, one stage per TIN-4668 scrubber pattern,
    // so the whole line (body included) is redacted before extraction.
    stage.replace { expression = "<TIN-4668 pattern>"  replace = "<redacted>" }
    stage.json { expressions = { op = "op", msg_id = "msg_id", thread_id = "thread_id",
                                 trace_id = "trace_id", span_id = "span_id",
                                 from = "from", to = "to", ticket = "ticket" } }
    stage.labels { values = { op = "" } }
    stage.structured_metadata { values = { msg_id = "", thread_id = "", trace_id = "",
                                           span_id = "", from = "", to = "", ticket = "" } }
    stage.static_labels { values = { service_name = "swb" } }
    forward_to = [loki.write.loki.receiver]
  }
  ```

  routed by a namespace `keep` in `discovery.relabel`, so the generic stage
  does not also handle these lines. The owner release writes the real block,
  and its redaction patterns are the TIN-4668 collector scrubber's set,
  kept in step with it (SWB-R34).
- Retention is Loki's 168h (SWB-R25). The Grafana Tempo datasource opens a
  span's Loki lines by `trace_id` with no new Grafana work
  (`monitoring.yaml:1869-1871`).

#### Mimir series (broker `:9090`, one static scrape job)

| Series | Labels | Bound |
| --- | --- | --- |
| `swb_sessions` | `harness`, `host`, `state` | ≤ 6 × 7 × 4 |
| `swb_session_last_seen_timestamp_seconds` | `harness`, `host` | ≤ 42 |
| `swb_mailbox_unacked` | `to_harness` | ≤ 6 |
| `swb_messages_total` | `from_harness`, `to_harness`, `transport` | ≤ 144 |
| `swb_push_notices_total` | `host` | ≤ 7 |
| `swb_claims_active` | `exclusive` | 2 |
| `swb_claim_overlaps`, `swb_handoffs_total` | none | 1 each |
| `swb_lgtm_export_failures_total` | `signal` (`tempo`, `loki`), `reason` | ≤ 8 |
| `swb_outbox_pending`, `swb_outbox_oldest_age_seconds` | `signal` | ≤ 2 each |
| `swb_projection_missing` | `signal`, `window` | ≤ 4 |
| `swb_projection_reconciled_timestamp_seconds` | none | 1 |

No long-lived series carries `pid`, `session_id` or `agent_id` (`agent_id`
is `harness:host:pid:session_id`, `schemas/envelope-v3.schema.json`
`$defs.agent_id`); per-session presence lives in the broker and in the
`swb.register` trace. The census textfile `swb_local_session{harness,pid,
session_id,socket_ok}` (ADR-0001, P3) is the one exception, accepted as
short-lived series. The scrape job is the single writer of `swb_*`; Alloy
does not scrape the broker (`registry.md:407-425`).

### Lookup recipes

Every harness that has the `grafana-tailnet` MCP reaches these: Claude
Code, Codex and OpenCode directly, Kimi through Claude Code, Junie only in
the mcp-mux `ops` group (`lab@3d755193 nix/lib/mcp-mux-manifest.nix:143-146`).
A switchboard-enrolled Junie session (`tracker`) or Pi session (`agents`
profile) had **no** LGTM tools in the September 25 baseline. SWB-R41 and
SWB-R48 now require an explicit combined path for both during P1b, backed
by L0 and measured tool budgets; existing default groups/profiles stay
intact. See [Enrollment](#enrollment-and-home-manager).
Truth comes first from the broker's own tools; LGTM corroborates and adds
history.

Time bounds every caller respects: Tempo search windows ≤ 168h with an
explicit `limit` (default 20); TraceQL metrics ≤ 3h on 2.7.2 and empty
until L4; Loki 168h; on Tempo 3.x subtract `query_end_cutoff` (30 s by
default) from "now" (Tempo `release-notes/v3-0.md:291-293`).

**Until L0 lands**, the only Tempo path is `grafana_api_request` `GET` on
`/api/datasources/proxy/uid/tempo/api/search?q=<TraceQL>&start=&end=&limit=`
and `/api/v2/traces/<hex>`; `find_slow_requests` is a Sift write tool
(Editor role) and is out of scope. After L0, the native tools are
`search_tempo_traces`, `get_tempo_trace`, `list_tempo_attribute_names`,
`list_tempo_attribute_values`, `query_tempo_metrics` and
`get_tempo_traceql_docs` (mcp-grafana `docs/sources/reference/mcp-tools-table.md:114-120`
at `v1.6.0`). `diff_tempo_traces` does not work against 2.7.2: it posts to
`/api/v2/traces/diff` (`tools/tempo.go:414`), a route that exists only on
Tempo `main`.

#### TraceQL (Tempo)

| Question | Query | Note |
| --- | --- | --- |
| One message, whole life | `{ resource.service.name = "swb-broker" && span.swb.msg_id = "<msg_id>" }` | Returns the send trace and each lifecycle trace |
| One message, direct | `get_tempo_trace(hex(sha256("swb:v1:msg:" ‖ msg_id)[0..16]))` | The skill computes the hash |
| Replies to a message | `{ resource.service.name = "swb-broker" && name = "swb.send" && span.swb.in_reply_to = "<msg_id>" }` | Or `{ link:spanID = "<send span id>" }`; whether 2.7.2 evaluates the `link:` scope is checked in L2's exit, and the attribute form works on any version. One query per hop; there is no cross-trace join |
| Messages in a thread, **sample only** | `{ resource.service.name = "swb-broker" && span.swb.thread_id = "<thread>" && name = "swb.send" } \| select(span.swb.seq, span.swb.from, span.swb.to, span.swb.ticket)` | Tempo returns the first N without pagination (a live `limit=200` query saturated at exactly 200); sort client-side by `swb.seq` and compare the set against `thread`'s max `seq` before trusting it. **Thread enumeration is broker-only** |
| Delivery corroboration | `{ resource.service.name = "swb-broker" && span.swb.msg_id = "<id>" && name = "swb.ack" }` | Ack truth is the broker's |
| Claims and handoffs on a ticket | `{ resource.service.name = "swb-broker" && span.swb.subject = "TIN-4655" && (name = "swb.claim" \|\| name = "swb.handoff") }` | Truth is `claims` |
| Who was active recently (corroboration) | `list_tempo_attribute_values("span.swb.from", q = {resource.service.name = "swb-broker"}, start, end)` | Observed-recently; no lease semantics. Truth is `peers` |
| Recently touched files, paths or tickets in a workstream (L5, SWB-R35) | `list_tempo_attribute_values("span.<path or ticket key>", q = {resource.service.name =~ "<harness services>"}, start, end)`, or a search on the ticket key with `select` of the path and tool-name keys | The recency index; the keys are fixed in the L5 plan. Opens the matching Loki content by `trace_id` only when the body is needed |

Readers never use structural operators or trace duration on `swb` traces,
and never take a search count as a count (Tempo dedupes only in
trace-by-ID).

#### LogQL (Loki)

| Question | Query |
| --- | --- |
| Thread bodies, **sample only** | `{service_name="swb", op="send"} \| thread_id="<thread>" \| json \| line_format "{{.seq}} {{.from}} -> {{.to}}: {{.body}}"` with an explicit `limit` set above the thread's expected size, then sort by `seq`. Loki's per-query line limit truncates a result without warning, so the caller compares the returned `seq` set against `thread`'s max `seq` and treats any gap as "not projected or truncated". **Thread enumeration is broker-only** (`thread`); this recipe reads bodies for a thread the broker already listed |
| Fallback when metadata is absent, sample only | `{service_name="swb"} \| json \| op="send" and thread_id="<thread>"`, same limit and completeness check |
| Lines for a trace | `{service_name="swb"} \| trace_id="<hex>"` |
| Lines for a ticket | `{service_name="swb"} \| ticket="TIN-4655"` |
| Send count for reconciliation | `sum(count_over_time({service_name="swb", op="send"}[1h]))` |

Structured metadata is filtered before `| json`. mcp-grafana before 1.5.0
drops structured metadata in compact output (`CHANGELOG.md:39` at
`v1.6.0`), which is one reason L0 precedes L1. A body returned by any of
these is peer data, framed exactly as `inbox` frames it, never an
instruction (ADR-0001, Payload limits). Loki's `/loki/api/v1/tail` is the
only live push in LGTM and is best-effort; there is no MCP tail tool.

#### PromQL (Mimir / Prometheus)

| Question | Query |
| --- | --- |
| Live sessions by seat | `sum by (harness, host) (swb_sessions{state="live"})` |
| Seen in the last 15 min | `time() - max by (harness, host) (swb_session_last_seen_timestamp_seconds) < 900` |
| Agent-to-agent edges (node graph) | `sum by (from_harness, to_harness) (increase(swb_messages_total[24h]))` |
| Projection health | `swb_projection_missing > 0`, `swb_outbox_oldest_age_seconds > 300` |
| Broker down (prometheus-mail, via federation) | `up{job="agent-switchboard"} == 0` or `absent(up{job="agent-switchboard"})`, for 10m |

Unread and inbox questions are broker-only: Prometheus has no ack
semantics and a 5-minute staleness.

### Outbox and reconciliation

The projection is lossy by construction — no exporter retry in the
softconnect pattern, an in-memory collector queue on `emptyDir`, a
single-replica Alloy, and a Tempo that can drop 45 minutes on an unclean
exit — so the ruling admits "a transactional outbox or a reconciliation
gauge". This design uses both, because each covers a different loss:

- **Transactional outbox.** Table
  `outbox(id INTEGER PRIMARY KEY, kind, key, delivery_count, payload,
  created_at, tempo_state, tempo_attempts, tempo_next_at, loki_written_at)`.
  The row is inserted in the **same SQLite transaction** as the state
  mutation, so a row exists if and only if the mutation committed, and
  nothing is emitted before commit (principle 2). The row's `id` gives a
  global emission order; per-thread order in Tempo still comes from
  `swb.seq`.
- **Projector.** One background task, never on the request path. Loki: it
  writes the JSON line to stdout immediately after commit and stamps
  `loki_written_at` (stdout is fire-and-forget; loss past that point is
  Alloy's, containerd's or Loki's and is measured, not retried). Tempo: it
  batches pending rows (≤ 32 spans, ≤ 64 KiB), posts to the collector with
  1 s connect and 2 s total timeouts, and on failure backs off from 1 s to a
  60 s cap. A row stays `pending` until `sent`, or becomes `abandoned` when
  the broker's own retention expires the underlying record (counted in
  `swb_lgtm_export_failures_total{signal="tempo",reason="abandoned"}`).
  Retries are safe because ids are deterministic: a re-sent span dedupes in
  trace-by-ID, and search counts are never truth.
- **Backpressure.** The broker keeps accepting sends however deep the
  outbox is; it raises `swb_outbox_pending` and `swb_outbox_oldest_age_seconds`
  and drops nothing of its own. The outbox is bounded only by the PVC.
- **Reconciliation** (proposed cadence: every 5 minutes, over the most
  recent complete one-hour window that ended at least 10 minutes ago). The
  broker reconciles **sets of ids, never counts**, because this ADR itself
  says a search count is never a count and a resend can appear twice in
  search. For the window `[t0, t1)` it takes its own set of committed
  `msg_id`s, then Tempo's set of distinct `span.swb.msg_id` values for
  `name = "swb.send"` gathered over half-open 5-minute slices
  `[s, s + 5m)` (so a trace on a boundary is counted once), each slice
  searched with `limit` = that slice's broker count + 1 so a saturated
  slice is detected and the whole window marked `unknown`, and Loki's set of
  distinct `msg_id` structured-metadata values for
  `{service_name="swb", op="send"}` over the same slices with the same
  limit rule. `missing` is the broker set minus the LGTM set; an overcount
  can never mask a loss because only the difference is exported. It
  exports `swb_projection_missing{signal,window="1h"}` (`-1` for
  `unknown`) and `swb_projection_reconciled_timestamp_seconds`, and writes
  one `reconcile` audit line with the three set sizes and the missing ids.
  `swb_lgtm_export_failures_total` counts exporter-side failures only; the
  gauge is what sees loss after the collector accepted. The reads need a
  tinyland.dev NetworkPolicy grant from the broker namespace to Tempo 3200
  and Loki 3100 (`tinyland-staging` is default-deny ingress,
  `network-policy.yaml:4-11`); that grant is an L2 owner item and exit.
- **Reader rule.** Absence in LGTM means "expired (7 d) or not projected";
  the skill says so and falls back to `thread`.

### Enrollment and Home Manager

Corrected against `lab@3d755193` (critique fixes adopted under R0 ruling 3):

- **Claude Code and Kimi.** The `agents` registry entry (`type: http`, the
  live Tailscale hostname, `targets` claude_code/codex/opencode true and
  default/vscode false, `codex_startup_safe` set deliberately, modeled on
  `pzm-computer-use`) reaches the harness as `vars/mcp_registry.yml` →
  `export-registries.py` → `tinyland.mcp` → `~/.claude.json`. It never goes
  through `settings.json` `mcpServers`: `tinyland.claudeCode.mcp.servers` is
  a "Legacy escape hatch" that warns (`nix/home-manager/claude-code.nix:512-514,689-692`),
  and activation prunes registry-managed servers out of `settings.json`
  (`claude-code.nix:927-928`). The contract test asserts the `~/.claude.json`
  projection. `kimi-claude.sh` exports `SWB_HARNESS=kimi` before its exec.
- **Claude hooks at `mkDefault`.** `tinyland.claudeCode.hooks` is
  `attrsOf (listOf anything)` (`claude-code.nix:634`); every existing definer
  merges at `mkDefault` through `mkMerge` (`claude-code.nix:751-772`, and
  the agent-process, GUI-launch, local-build, serena, worktree-discipline
  and atuin modules). One normal-priority definition would win outright and
  discard them all, including the three fail-closed host-loss guards. The
  switchboard hooks (SessionStart, UserPromptSubmit, Stop, SessionEnd; fixed
  store-path launchers of `swb hook claude <event>`, timeout 2) therefore
  land at `mkDefault` behind `tinyland.claudeCode.switchboard.enable`, with a
  contract test that, with it enabled on neo's config, the PreToolUse lists
  still contain the three envelope paths and the agent-process hook.
- **Codex.** `notify` is a runtime-owned top-level scalar, not a table:
  the converge writer declares only `mcp_servers`, `projects`,
  `sandbox_workspace_write` and the optional `computer_use`
  (`scripts/lib/codex_toml_writer.py:90-99`), handles scalars separately
  (`:171`), and neo's live config already carries
  `notify = [".../SkyComputerUseClient", "turn-ended"]`
  (`docs/agent-notes/2026-09-04-sess-587af118-computer-use-bridge-b-design.md:128-129`).
  Declaring it would displace the computer-use hook, the same way owning
  `features` would overwrite operator toggles. So: no new table, Codex
  registers through `swb whoami`, heartbeats ride tool calls, and the
  registry projects through `export-registries.py` and the Codex inventory
  cassettes. A fan-out `notify` wrapper is a separate ruling if ever wanted.
  **The binding mechanism is unproven.** No research lane produced evidence
  of how `swb whoami`, run as a Codex MCP tool process, picks the calling
  session when several Codex sessions run on one host; the earlier phrase
  "reads Codex's own session records read-only" named a place, not a
  binding. The binding must be something Codex hands the process — an
  environment variable or a per-session path it exposes — recorded with
  evidence in the P1b receipt. It is never derived by walking pid ancestry:
  classifying a process by its ancestors is the pattern the R-N11 corollary
  names (lab `docs/operations/STING_FIRST_HOUR.md`, "Process safety"). Until
  the binding is evidenced, Codex registration is an unproven leg with its
  own observable exit in ADR-0001 → P1b, and the P1b round trip does not
  silently depend on it.
- **OpenCode.** The same registry entry through `opencodeRegistryServers`;
  identity via `swb whoami`.
- **Junie.** One server per launch: the default group `mux`, or a profile
  group as "an OPT-IN override per launch … each budgeted alone"
  (`mcp-mux-manifest.nix:31-32`). `ag` goes in `tracker` only (SWB-R12;
  `tracker` = lin + gh reads, measured 87, plus 11 = 98 ≤ 100). In the
  September 25 baseline, `graf` was in `ops` only (`:143-146`), `mux` had
  neither, and no single session had both broker and LGTM reads. A
  combined path is now required in P1b (SWB-R41), with an explicit read
  allowlist and measured combined budget. Preserve the existing default
  groups.
- **Pi.** `zero` is the ratified default profile and a launch selects one
  profile (`vars/mcp_registry.yml:150-262`, TIN-3017). The `agents` profile
  (SWB-R18) uses `source_only`, `runtime_admission_required` and its own
  launcher. The September 25 proposal excluded Grafana, and the only
  Grafana profile, `infrastructure` (`:224`), had no runtime launcher. SWB-R37 places `agents`
  in P1b with registration and a threaded round trip; SWB-R41 adds the
  explicit combined broker/LGTM path in P1b, preserving `zero` and
  measuring the combined budget. The path depends on usable L0 reads.
- **agentd** (P2, lab `tinyland.swbAgentd`): launchd on PZM and neo, systemd
  user unit on honey, sting and bumble; metadata-only telemetry under
  SWB-R40/SWB-R47, with source proof under SWB-R44 after v1 (SWB-R48);
  census textfiles only where a writable path is declared. On neo and PZM
  `/nix` is external,
  and a `/nix`-hosted LaunchDaemon never started on neo
  (`nix/darwin/modules/node-exporter-darwin.nix:14-33`): stage on the
  internal volume or prove user-domain exec and reboot survival as a P2 exit.
- **False artifact.** `settingsJson.telemetry.enabled` (`claude-code.nix:187-188`)
  has no documented upstream key and is not in `managed_keys` (`:813`). It is
  verified against upstream and removed when P1b next touches
  `claude-code.nix`, or the reason it stays is recorded, per "remove false
  artifacts as you go". The target state for harness telemetry is SWB-R32,
  which makes the real gate `CLAUDE_CODE_ENABLE_TELEMETRY` plus the
  `OTEL_*` variables, rendered from user scope (the only scope Claude Code
  honors for telemetry export variables).
- **Contract tests** (registry entry shape and URL string, hook priority and
  timeout invariants, agentd neo bounds, mcp-mux `tracker` budget,
  mcp-grafana version floor) land with `tests/BUILD.bazel` wiring and a
  `test/manifest.json` `ci_evidence` entry in lab. They never run as a
  local `bazel test` on neo (the local-build guard). **lab has no named
  remote test lane** at `3d755193`: `git grep remote-check` finds no
  justfile recipe (only an unrelated `fail remote-check` string in
  `scripts/validation/sting-fast-local-storage-check.sh:274` and a
  `docs/DEPLOYMENT.md:150` line about Home Manager), and `just test-unit` /
  `just test-bazel` / `just test-presubmit` (`justfile:789-846`) are local
  recipes. `just remote-check` is this repo's recipe (`justfile:34-38`,
  rsync plus `just check` on sting) and covers this repo only. So the lab
  by-hand lane was the September 25 fallback. SWB-R39 (2026-09-26) now
  requires a named lab remote entrypoint targeting Honey first, Sting as
  fallback, before the first P1b contract test. Each receipt records exact
  source and execution host; this repo's `just remote-check` remains a
  separate recipe, not evidence that lab's named lane is delivered.
- **Delivery is per host and verified.** neo: the generation is produced on
  the bounded remote producer and activated through the signed-closure
  path, never compiled locally. PZM: `just nix-switch petting-zoo-mini` onto
  the selected external USB `/nix`, Home Manager only, never bundled with a
  `darwinConfigurations` change. honey, sting, bumble: `just nix-switch
  <host>` or `just fleet-switch-local`. After every switch the host's own
  session appears in `ag peers` and a threaded round trip completes, in a
  dated TIN-4655 comment (R-N13). The host order is Neo Claude ↔ Sting
  Codex P1b acceptance, then Honey/Bumble, then yoga/mbp-13 (SWB-R38,
  SWB-R45). PZM remains behind its storage delivery gate. This is an
  enrollment order, not evidence that any host was switched.

### Harness-native telemetry (L5)

Ruled 2026-09-25 (SWB-R31, SWB-R32): "all on, all being shipped"; "Full
content, scrubbed at the collector"; ingest "Request it from
tinyland.dev/blahaj", filed as TIN-4668. Amended the same day (`f870729e`):
Tempo is the harness recency index and content stays in Loki (SWB-R35),
and L5 is independent of P4 and starts once its three gates hold
(SWB-R36).

- **What ships.** Claude Code and Kimi first: `CLAUDE_CODE_ENABLE_TELEMETRY=1`,
  `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`, metrics, log events and traces
  over OTLP/HTTP, and **all five** content gates the harness-telemetry lane
  lists **on** (`OTEL_LOG_USER_PROMPTS`, `OTEL_LOG_ASSISTANT_RESPONSES`,
  `OTEL_LOG_TOOL_DETAILS`, `OTEL_LOG_TOOL_CONTENT`,
  `OTEL_LOG_RAW_API_BODIES` — "all on" leaves none out), rendered from
  user-scope settings by Home Manager. Codex second: the `[otel]` table with `log_user_prompt = true`
  and the OTLP/HTTP exporter, declared only if the converge writer gains an
  `otel` optional table under the `computer_use` precedent — `[otel]` is
  runtime-owned today (enrollment-hm lane, `codex_toml_writer.py:90-99`).
  Kimi inherits Claude Code's rendering verbatim (`kimi-claude.sh` execs
  the Claude CLI). OpenCode's experimental AI SDK spans carry full prompt
  text with no redaction toggle and need a plugin to export at all; under
  R31 that content is scrubbed at the collector like everything else.
  Junie's telemetry is proprietary and goes to JetBrains, not to any OTLP
  endpoint the fleet controls. Pi has a telemetry schema but ships nothing
  without an adapter.
- **Scrubbing** happens in the collector, not on the host: TIN-4668's
  redaction processor strips API tokens (GitHub, Linear, Anthropic,
  OpenAI), private keys, age identities, SSH keys, sops-decrypted value
  patterns and bearer headers from attributes, bodies and events before
  export, with its own tests.
- **Gates, named exactly.** lab renders the exporters gated off until
  **the scrubbed endpoint** exists — the TIN-4668 Service
  `otlp-harness-http-tailscale` (port 4318 → target 14318, receiver
  `otlp/tailnet`, `875fc21f`) — with its redaction processor and its logs
  pipeline, and until the body-read ACL (SWB-R27), which "gates it too", is
  enforced: SWB-R36 names L5's third gate "the enforced ACL", so ACL A
  applied by TIN-4670, the same bar SWB-R33 sets for `swb` bodies.
  "Until that endpoint exists" does **not** mean any 4318: the
  retained `otlp-observability-http:4318` exists today and bypasses the
  scrubber, and the operator's word is that lab keeps "the harness
  exporters gated off, and never to point them at the old
  `otlp-observability-http:4318`" (`1001c0fb`; TIN-4670 repeats it). The
  rendered `OTEL_EXPORTER_OTLP_ENDPOINT` therefore names the scrubbed host
  and nothing else, and L5's exit checks the rendered value. Whether the
  old 4317/4318 Services are retired or repointed is the operator's pick on
  TIN-4670 after the scrubbed endpoint deploys; until that pick lands, the
  old path is a remaining bypass that any tailnet host could use by name,
  and this ADR states it rather than closing it. This supersedes
  `tinyland.claudeCode.telemetry.enable = false` as the target state, "but
  not until the endpoint and scrubber exist" (SWB-R32). Host-side OTLP
  otherwise stays where TIN-75 left it; the prompt-pulse "OTLP stays gated
  until TIN-75" line (`nix/home-manager/prompt-pulse.nix:31-49`) is
  prompt-pulse's own coupling, not a host-wide gate (critique correction).
- **Where harness content lives.** Tempo truncates every attribute at
  2048 bytes (`configuration/_index.md:240` at `v2.7.2`), so full prompt
  or tool content can never be observed in Tempo, and the in-flight
  TIN-4668 build enforces ruling 2's "never into Tempo span attributes" on
  `traces/tailnet` (`1001c0fb`; `875fc21f`: "The traces pipeline would
  need a key allowlist"). Under this ADR the content store for L5 is
  therefore **Loki, through `logs/tailnet`** (Claude Code's log events and
  Codex's `codex.user_prompt` events), and Tempo carries the harness trace
  skeleton plus the recency attributes — tool names, file paths, ticket IDs
  — and never a body ([Harness span attributes](#harness-span-attributes-l5-swb-r35);
  SWB-R35). The operator's answer keeps content out of Tempo and gives
  Tempo a positive role as the recency index.
- **Resource attributes** stay host, harness and seat as sent; the
  collector's `k8s.cluster.name` / `service.namespace` upsert must not
  relabel host telemetry (observed on softconnectd spans), which TIN-4668
  carries as its own requirement. Metrics from harness OTel pass through
  `resource_to_telemetry_conversion` (`otel-collector.yaml:44-45`), which
  turns every resource attribute into a label; L5's exit bounds the series
  growth.

### Phases L0–L5

Interleaved with the P phases as R0 → P1a → L0 → the SWB-R27 decision →
P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4 (SWB-R28, adopting the plan's
order), with **L5 off that sequence**: it is independent of P4 and starts
once its three gates hold — the TIN-4668 scrubbed endpoint, its scrubber
and the enforced ACL (SWB-R36). The SWB-R27 decision is recorded (ACL A,
SWB-R33); the step stays in the order as the point before P1b where it had
to exist.
*Clarified 2026-09-26 (SWB-R41, SWB-R48):* L0's usable read plane is a
functional prerequisite for P1b's combined Junie/Pi path. Only telemetry
proof — collector scrubbing and enforced source provenance — follows v1
functional proof and must not block the broker MVP. Message bodies remain
disabled until SWB-R33's live ACL and SWB-R34's redaction gates pass.
Each L
phase writes a `docs/agent-notes/` entry and a dated TIN-4655 comment
(R-N13). Every exit below is observable: a query, an HTTP status, a file, a
series, a receipt. **Every drill that stops, restarts or scales a process
is performed by the operator** (R-N11: agents never signal a process, on
any host); the agent asks, waits, and records the result.

#### L0 — read plane in lab (after P1a; gated by the Host-header canary, SWB-R29)

- **Owner:** `xoxd-ai/lab` (`nix/packages/mcp-grafana.nix`,
  `nix/modules/grafana-proxy.nix`, `nix/lib/mcp-mux-manifest.nix`);
  `Jesssullivan/tailnet-acl` only if the bound Grafana URL is retargeted.
- **Gate, first.** mcp-grafana 1.2.0 inherited mcp-go's default-on DNS
  rebinding protection, "which returns 403 when a request arrives over a
  loopback connection with a non-loopback `Host` header — a second check …
  that `--allowed-hosts` cannot loosen" (`CHANGELOG.md:136` at `v1.6.0`).
  Honey's proxy is exactly that shape: bound to `127.0.0.1`, published by
  `tailscale serve`, which "forwards this url's host unchanged"
  (`grafana-proxy.nix:90,120`; `vars/mcp_registry.yml:921-923`). The 1.4.0
  entry says "`--allowed-hosts` is now honored when the request arrives via
  a loopback reverse proxy" (`CHANGELOG.md:85`), so the outcome is
  uncertain and the canary decides. Canary: run the candidate (1.5.1 or
  1.6.0) on honey on a spare loopback port published through
  `tailscale serve`, and from a seat issue a bare `GET
  http://honey.taila4c78d.ts.net:<port>/mcp`. `405`/`406`, or `200` with
  `Content-Type: text/event-stream`, means the transport accepted that
  Host. A `200` with another content type does not pass. Bare GET success
  alone does not establish MCP readiness: JSON-RPC `initialize` must
  return the candidate mcp-grafana version, and `tools/list` must include
  `search_tempo_traces` and `get_tempo_trace`. The successful Tempo calls
  and budget checks below remain required for full L0 acceptance.
  `403` rejects that Host; pick a Host strategy before the bump — bind to
  honey's exact tailnet IP (the node-exporter-darwin exact-bind precedent)
  or put a Host-rewriting loopback proxy in front — then repeat the probes.
  Record HTTP status and content type, initialize/version and tool-list
  evidence in the receipt. The canary is a unit the operator starts and
  later stops (R-N11); the agent records both.
  *Corrected 2026-09-27 UTC (SWB-R29; no ruling change):* the previous
  wording named only 405/406. Lab's canary already accepts 200 SSE and
  separately checks initialize (`lab@430f6e37ed9657478f61d2708ff56bb7b3d35340`
  `scripts/validation/mcp-plane-canary.py:5-14,97-105,249-274,315-333`).
  TIN-4655 comment `750738aa-3fda-4828-9857-a934c2a20920` records the
  read-only canary packet and this interpretation; it claims no live L0
  acceptance.
- **Scope.** Bump 0.17.2 → ≥ 1.5.0 with the four platform hashes
  re-prefetched; pin `--usage-stats=disabled` explicitly (added in 1.5.0,
  `CHANGELOG.md:34`); keep the Viewer service account. **Measure the `ops`
  tool count on the canary before the honey switch** (the critique's fix,
  carried whole): with the canary running, honey's mcp-mux provisioning
  step is pointed at the canary port and re-measures `ops` into
  `~/.local/state/tinyland/mcp-mux/status.json`; the honey switch is
  blocked until that measurement passes (≤ 100 tools, every composed name
  ≤ 64 characters). If it fails, `ops` switches to an explicit `graf`
  `includedTools` list first (the plan's 82 → 88 arithmetic was
  unverified; the 1.6.0 tools table adds seven Tempo tools plus others).
  Record the three 2026-09-25 timeouts against `100.74.127.80:3000` and
  the bound URL's move to the canonical Ingress. SWB-R42 (2026-09-26)
  selects retargeting only after the precise tailnet ACL grant for Honey
  and the live Host-header canary (`kubernetes.dhall:64-67` grants only
  the L4 address in the baseline). Proxied Tempo tools wait for a separately
  measured allowlist.
- **Exit, all must hold:**
  - the allowed Host returns 405/406 or 200 SSE on bare GET; any 403 is
    resolved by the chosen Host strategy and re-probed. Record status and
    content type; an unlisted Host is rejected with 403;
  - JSON-RPC `initialize` identifies the candidate mcp-grafana version,
    and `tools/list` includes `search_tempo_traces` and `get_tempo_trace`;
    bare GET transport acceptance alone cannot satisfy this exit;
  - the pre-switch `status.json` measurement against the canary shows `ops`
    at ≤ 100 tools and every composed name ≤ 64 characters, and that
    receipt predates the honey switch;
  - from a Claude Code seat over the direct tailnet path,
    `search_tempo_traces` with `{resource.service.name="softconnectd"}`
    returns at least one trace and `get_tempo_trace` returns it;
  - from a Junie `ops` launch through mcp-mux, the same two calls succeed;
  - after the switch, `status.json` on honey still shows `ops` at ≤ 100
    tools and every composed name ≤ 64 characters;
  - the running unit's argv contains `--usage-stats=disabled`, and its
    reported version equals the pin, read back on honey after
    `just nix-switch honey` (landing is not delivering);
  - the operator has stopped the canary unit and the receipt says so.

#### L1 — Loki bodies and audit via Alloy (after P1b; SWB-R19, R25, R26, R27, R33, R34)

- **Owner:** tinyland.dev owner release (`infra/staging/alloy.yaml`
  namespace allowlist and the `loki.process` block with its redaction
  step; RBAC if the new namespace needs it), sequenced by blahaj; this repo
  (line schema); `Jesssullivan/tailnet-acl` (the public Dhall repo) for
  ACL A, applied by TIN-4670.
- **Scope.** The broker namespace is added to the Alloy allowlist
  (`alloy.yaml:326-345`) with the `loki.process` stage above, including the
  redaction step with the TIN-4668 collector scrubber's pattern set
  (SWB-R34); the broker emits the line schema; bodies appear only once ACL
  A is enforced live (SWB-R33).
- **Gate for bodies (SWB-R33, SWB-R34).** Bodies ship only when both hold:
  TIN-4670 has applied ACL A in `Jesssullivan/tailnet-acl` and a raw Loki
  read from a non-admin tailnet node is refused; and the redaction step is
  live in the broker namespace's `loki.process` stage. *Proposed, not
  ruled:* L1 may close on its metadata exits with bodies off, and the body
  exits below then run as their own dated receipt once the gate holds, so
  L2 does not wait on TIN-4670.
- **Exit, all must hold:**
  - `query_loki_logs` `{service_name="swb", op="send"} | thread_id="<P1b thread>"`
    returns the P1b round-trip lines with `msg_id` and `trace_id` present
    as structured metadata, and the `| json` fallback returns the same
    lines;
  - `list_loki_label_names` for `{service_name="swb"}` shows exactly
    `service_name`, `op` and the pod labels — no `msg_id`, `thread_id` or
    `trace_id` label;
  - the broker binary has no Loki client and no OTLP logs exporter (a
    code-level assertion), and `list_loki_label_values(job)` shows `swb`
    lines only under `loki.source.kubernetes.pod_logs`. That a third party
    could push forged lines to the unauthenticated tailnet endpoint is
    accepted and stated, not disproved (the critique's point that job
    labels are client-set);
  - the SWB-R27 decision is on TIN-4655 as a dated comment — **met
    2026-09-25**: comment `f870729e` records that the operator's TIN-4668
    answer, "A: tailnet ACL, admins + MCP" (`1001c0fb`), is SWB-R27's
    decision (SWB-R33). The decision's scope covers every
    reader: every tag and group that reaches `tinyland-loki-observability:3100`,
    `tempo-observability:3200` (unauthenticated, `875fc21f`),
    `tinyland-grafana-observability:3000` and the canonical Grafana Ingress
    — naming `group:dollhouse-admins` and `group:dollhouse-users`
    (`kubernetes.dhall:43-46`) and the `tag:k8s-operator` grant
    (`kubernetes.dhall:39-42`) explicitly — every Grafana user (OSS Grafana
    has no per-datasource permissions, `875fc21f`), the Grafana Viewer
    service account behind the MCP proxy, and `kubectl logs` on the broker
    namespace;
  - **decided is not enforced**, ruled 2026-09-25 (SWB-R33, "Once ACL A is
    enforced live (Recommended)"). Before any body ships, the receipt shows
    a raw Loki read (`/loki/api/v1/labels` on
    `tinyland-loki-observability:3100`) from a non-admin tailnet node
    refused, and the same read from an admin node and from the MCP proxy
    succeeding; the Tempo 3200 read is recorded the same way. Until then
    L1 ships no bodies.
  - **Body exits, once the gate holds:**
    - a canary `send` whose body carries one synthetic token in each
      TIN-4668 scrubber shape arrives in Loki with every token redacted,
      and the `size`/`body_hmac` on the line and on the span are those of
      the unredacted body (the broker hashes before Alloy redacts)
      (SWB-R34);
    - a maximum-size line (16 KiB body plus metadata) arrives intact, which
      settles the unverified containerd CRI 16 KiB line-split question.

#### L2 — Tempo dialog projection, write-through from the broker only (after L1)

- **Owner:** this repo (exporter, outbox, reconciliation); tinyland.dev
  owner release (two NetworkPolicy grants for the broker namespace: ingress
  to `otel-collector` 4318 for the exporter, and ingress to `tempo` 3200
  and `loki` 3100 for the reconciliation reads, both into default-deny
  `tinyland-staging`; optionally `k8sattributes`), sequenced by blahaj;
  blahaj (the broker-held HMAC key as a sops leaf).
- **Scope.** The OTLP http/protobuf exporter to
  `otel-collector.tinyland-staging.svc:4318`, the message, lifecycle, claim
  and session traces of the data model, the transactional outbox and the
  reconciliation job; `swb_lgtm_export_failures_total` and the projection
  gauges exported. No host and no agentd emits `swb` spans.
- **Exit, all must hold:**
  - from the broker pod, a request to the collector's 4318 gets an HTTP
    answer, not a timeout (the exporter grant is live), and a request to
    Tempo `/api/search/tags` and Loki `/loki/api/v1/labels` each gets an
    HTTP answer (the reconciliation grant is live);
  - for an acked P1b message, `search_tempo_traces`
    `{ span.swb.msg_id = "<id>" }` returns the send trace and the ack trace
    as two trace ids; `get_tempo_trace` on the ack trace shows a link whose
    trace id and span id equal the send span's; the reply's send span links
    to the original send span; and whether the `link:spanID` query form
    returns the reply on 2.7.2 is recorded;
  - `tracesToLogs` from the send span opens the Loki line for that
    `trace_id` in Grafana;
  - `list_tempo_attribute_values("resource.service.name")` includes
    `swb-broker`. SWB-R44 requires collector proof of source identity and
    rejection of spoofed client identity for broker and agentd lines,
    spans and series. Enrichment alone does not pass. This proof follows
    v1 functional proof (SWB-R48); broker tools remain authoritative;
  - reconciliation drill (the operator makes the collector unreachable
    from the broker, by the NetworkPolicy or by scaling the collector,
    and restores it; the agent sends and measures): with the path down for
    at least 10 minutes while at least 20 messages are sent, `send` p99
    stays within 10 ms of its P1b baseline (a proposed bound, stated
    before the drill); after the path is restored
    `swb_outbox_pending{signal="tempo"}` returns to 0 within 5 minutes and
    `swb_projection_missing` for that window returns to 0 at the next
    reconciliation;
  - dedupe drill: one message redelivered with `delivery_count` 1 and 2
    yields two `swb.fetch` traces with distinct span ids; and, after the
    operator restarts the broker mid-drain (R-N11), for every message whose
    outbox row shows `tempo_attempts` > 1: the reconciliation set diff for
    that window is empty (no send is missing), trace-by-ID shows one
    `swb.send` span, and the raw `search_tempo_traces`
    `{ span.swb.msg_id = "<id>" && name = "swb.send" }` result and the
    `span.swb.msg_id` tag-values count are recorded — that is where a
    resend can appear twice, and the receipt says whether it did. (The
    earlier "no duplicate send span in trace-by-ID" could never fail,
    because trace-by-ID dedupes by construction.)

#### L3 — presence and graph metrics, dashboard, alert (after P2; SWB-R28)

- **Owner:** tinyland.dev owner release (the static scrape job in
  `monitoring.yaml` and the file-provisioned `swb` dashboard ConfigMap);
  `xoxd-ai/blahaj` (the ingress policy that admits the staging Prometheus
  into the broker's namespace on `:9090` — an ingress rule into the
  broker's namespace belongs to blahaj's broker stack, not to the
  tinyland.dev owner release; the prometheus-mail rule and the federation
  match for `up{job="agent-switchboard"}` and `swb_*`, in the
  `network-federation.yml.tftpl` precedent). lab authors none of it.
- **Exit, all must hold:**
  - `count({__name__=~"swb_.*"})` in Mimir is below 1,000, and no `swb_`
    series other than `swb_local_session` carries a `pid`, `session_id` or
    `agent_id` label;
  - `sum(swb_sessions{state="live"})` equals the `ag peers` live count
    within 5 minutes;
  - `count by (job)(up{job="agent-switchboard"})` in Mimir is 1 (one
    writer, the double-count rule);
  - prometheus-mail's `/api/v1/query` returns `up{job="agent-switchboard"}`
    through the federation match, and after the operator scales the broker
    to zero (R-N11; a blahaj action the operator performs) the "broker
    unreachable 10m" rule fires and the notification reaches the mail
    stack's ntfy sink (`alertmanager_ntfy_topic`, `blahaj@2071613c
    tofu/stacks/mail/modules/monitoring/main.tf:33,305`; the estate-lgtm
    lane read its value as `blahaj-mail-alerts`; the tfvars value is
    confirmed in the receipt); the operator then scales it back;
  - the dashboard renders the P1b thread as a node graph from
    `swb_messages_total`.

#### L4 — Tempo owner-release upgrade (after P3)

- **Owner:** tinyland.dev owner release, sequenced by blahaj; requested by
  dedicated linked ticket [TIN-5022](https://linear.app/tinyland/issue/TIN-5022)
  under SWB-R43 (2026-09-26), with no delivery date implied. An Opus 5.5
  refutation pass on the upgrade plan
  precedes scheduling (the plan's routing, recorded in R0).
- **Scope.** Tempo 2.7.2 → 3.x monolithic (no Kafka;
  `tempo-cli migrate config --mode=monolithic`), or 2.9/2.10 as an interim;
  `query_frontend.mcp_server.enabled` and `stream_over_http_enabled`;
  metrics-generator span metrics with `remote_write` to Mimir (service
  graphs optional: the 10 s pairing `wait` cannot pair mailbox latency);
  dedicated columns for `swb.*` (vParquet5 for an integer `swb.seq`);
  `query_end_cutoff` set deliberately, because the 30 s default delays
  every chat view; `max_span_attr_byte` left at 2048 (bodies stay in Loki);
  the unclean-exit loss window re-measured under the live-store. The stack
  stays `migration_blocked` until the owner-release evidence lands.
- **Exit, all must hold:** `tempo_build_info` reports the new version;
  `GET …/tempo/api/mcp` answers and, if proxying stays on, `tempo_*` tools
  appear beside the native ones; `query_tempo_metrics`
  `{resource.service.name="swb-broker"} | rate()` returns series; the
  measured search freshness against `query_end_cutoff` is recorded; the
  block format version is recorded; the TIN-4655 comment cites the
  owner-release PR.

#### L5 — harness-native telemetry (SWB-R31, SWB-R32, SWB-R35, SWB-R36; independent of P4, starts once the scrubbed TIN-4668 endpoint, its scrubber and the enforced ACL all exist)

The operator's answer, "Start once its three gates hold (Recommended)"
(SWB-R36), makes L5 independent of P4; it can start before, during or
after any P phase once the gates below hold. *Clarified 2026-09-26
(SWB-R48):* telemetry qualification is post-v1 functional proof and cannot
hold the P1b broker MVP; the existing body gates still apply.

- **Owner:** tinyland.dev owner release (the collector is tinyland.dev's:
  the TIN-4668 receiver `otlp/tailnet` on 14318, Service
  `otlp-harness-http-tailscale` 4318 → 14318, redaction processor,
  `traces/tailnet` key allowlist and `logs/tailnet` pipeline; the PR is
  being built by the blahaj seat), sequenced by blahaj; `xoxd-ai/lab`
  (exporter rendering, Claude Code and Kimi first, then Codex; host order
  after Neo–Sting acceptance is Honey/Bumble then yoga/mbp-13, with PZM
  behind storage delivery, SWB-R38/SWB-R45);
  `Jesssullivan/tailnet-acl` (public Dhall) for ACL A and any grant the
  endpoint needs; `xoxd-ai/blahaj` (TIN-4670, the old Services'
  disposition).
- **Gates, all before the flip (SWB-R36):** the scrubbed endpoint answers;
  its scrubber's tests pass; ACL A is enforced — TIN-4670 has applied it in
  `Jesssullivan/tailnet-acl` and a raw Loki read from a non-admin tailnet
  node is refused (the SWB-R33 bar), with SWB-R27's decision carried on
  TIN-4655 (`f870729e`) and covering the readers named in L1. Also, the rendered
  `OTEL_EXPORTER_OTLP_ENDPOINT` (and any signal-specific
  `OTEL_EXPORTER_OTLP_*_ENDPOINT`) on every enrolled host equals the
  scrubbed host and never `otlp-observability-http` or
  `otlp-observability-grpc` (`1001c0fb`), asserted by a lab contract test
  on the rendered environment.
- **Exit, all must hold:**
  - `GET` on the scrubbed endpoint's `/v1/traces` and `/v1/logs` returns
    405 (both handlers exist), and the old `otlp-observability-http:4318`
    `/v1/logs` still returns 404 or the Service is gone (TIN-4670's
    disposition recorded either way);
  - the rendered exporter endpoint on neo, read back from the live
    environment of a fresh Claude Code session, equals the scrubbed host;
  - **content, checked in Loki:** from neo, a canary prompt sent through a
    Claude Code session appears as a `claude_code.user_prompt` log event
    in Loki through `logs/tailnet` with its prompt text present, and the
    matching `claude_code.interaction` trace is visible in Tempo with the
    harness skeleton and with no attribute carrying the prompt text (Tempo
    cannot hold it: 2048-byte attribute cap and the `traces/tailnet`
    allowlist);
  - **recency, checked in Tempo (SWB-R35):** for a canary Claude Code
    session that reads one known file and names one known ticket, a Tempo
    query over the last hour returns that session's spans carrying the tool
    name, the file path and the ticket ID as span attributes, and no span
    attribute of that trace carries a prompt, tool input, tool output or
    response body;
  - **scrub, checked on both signals:** a canary prompt containing one
    synthetic token in each scrubbed shape (`875fc21f`'s pattern list)
    shows every token redacted in the Loki event and in every Tempo span
    attribute, event and status message of that trace;
  - Codex events (`codex.user_prompt` with content) reach Loki through
    `logs/tailnet`, and the same scrub check passes for a Codex canary;
  - a `swb.send` span emitted for a message sent with `swb send` from a
    Claude Code Bash tool carries a link that resolves to the harness tool
    span. Because L5 no longer waits for P4 (SWB-R36), it can run before
    L2 emits `swb` spans; if so, this one sub-exit is recorded as pending
    (proposed, not ruled) and closed by a dated receipt once L2 has landed;
  - the Mimir series attributable to harness resource attributes
    (`resource_to_telemetry_conversion`) are counted and stay under
    **10,000 across all enrolled hosts over the first 7 days** (a proposed
    bound, 1% of Mimir's 1 M per-tenant cap, stated here so the drill has a
    number; owner lab, confirmed or replaced in the L5 plan comment before
    the flip); if it fails, the per-session attribute is dropped from the
    metrics resource before enrollment continues;
  - the `settingsJson.telemetry.enabled` key has been removed or its
    retention recorded;
  - the receipt cites SWB-R31, SWB-R32, SWB-R35 and SWB-R36.

## Risks

- **A single read plane.** The grafana-tailnet MCP is one process pair on
  honey, bound to the legacy L4 Grafana IP, Viewer role, with its token only
  in honey's sops store (`grafana-proxy.nix:90,120`; `registry.md:586-600`);
  it timed out three times on 2026-09-25. The peer-dialog skill degrades to
  the broker and to lab #1920 and never blocks on LGTM.
- **The L0 bump may return 403 to every tailnet client** while mcp-mux
  (loopback) keeps working, which is why the canary checks both paths
  (`CHANGELOG.md:136,85` at `v1.6.0`).
- **Unauthenticated writes and reads, today.** Any tailnet peer the ACL
  admits can push spans and metrics into the unscrubbed in-cluster
  pipelines through the retained `otlp-observability-{grpc,http}` Services
  under any claimed identity, and can read every Loki stream and every
  Tempo trace with no credentials (`registry.md:293`;
  `retained-observability-tailnet-services.yaml:58-125`;
  `kubernetes.dhall:39-46,60-67`; probes in `875fc21f`). LGTM dialog records
  require proved source identity under SWB-R44 before being treated as
  broker or agentd output; filtering `resource.service.name` or enriching
  attributes alone does not establish that. SWB-R48 places this proof
  after v1 functional acceptance and keeps consequential state in broker
  tools. Closing the write path and
  applying read ACL A is TIN-4670, sequenced after the TIN-4668 build;
  until it lands the old 4317/4318 names remain a bypass of the scrubber
  for any host that uses them, and lab's exporters never do (`1001c0fb`).
- **Bodies and egress.** Once bodies land in Loki, everyone L1's audit
  scope names — `group:dollhouse-admins`, `group:dollhouse-users`, the
  `tag:k8s-operator` grant, every Grafana user, the Viewer service account
  and `kubectl logs` — can read them, and a central lookup lets any
  harness pull any thread's bodies into its own model provider (Kimi's
  endpoint, OpenAI for Codex, JetBrains for Junie), bypassing the
  recipient-only path. SWB-R27 gates this, and bodies wait for ACL A to be
  enforced live (SWB-R33); the skill frames LGTM bodies as peer data;
  serving bodies only through the broker with LGTM holding `body_hmac` is
  the alternative if the audit says no. The read-ACL narrowing is the
  operator's own answer ("A: tailnet ACL, admins + MCP", `1001c0fb`) and
  lands through TIN-4670 in `Jesssullivan/tailnet-acl`; this ADR proposes
  no narrowing of its own and asks for none. ACL A narrows raw Loki and
  Tempo reads; it leaves every Grafana user and the MCP proxy able to read
  bodies, which is the audited-user-list half of that answer.
- **`swb` bodies and the scrubber.** They reach Loki through stdout →
  Alloy, never through the TIN-4668 collector redaction processor, and the
  broker does not redact (SWB-R19). Ruled 2026-09-25 (SWB-R34): the broker
  namespace's `loki.process` stage gets a redaction step with the same
  pattern set. What remains: pattern redaction strips only known secret
  shapes, the two pattern sets (collector and Alloy) can drift apart unless
  the owner release keeps them in one source, and a secret in an unknown
  shape still reaches Loki — so SWB-R19's "never put a secret in a body"
  stays the first line.
- **A keyed hash, not a bare one.** With Tempo and Loki readable without
  credentials, a bare `sha256(body)` beside `size` would let any reader
  confirm a guessed short body ("yes", "ack", "done") by hashing it. The
  design therefore uses `body_hmac` under a broker-held key on both
  signals; the exposure that remains is `size` itself, which is stated
  here and accepted for a 1-byte-granular length.
- **Availability.** Tempo, Loki, Mimir, Pyroscope and Alloy are
  single-replica `Recreate` deployments on sting; Grafana is single-replica
  on bumble with Live disabled; the stack is `migration_blocked` with
  `emptyDir` runtime state. Hooks and delivery never depend on it.
- **Tempo limits that silently corrupt a chat view:** 2048-byte attribute
  truncation, 5 MB per trace, 10,000 live traces, default limit 20, first-N
  ordering, non-deterministic `most_recent`, no pagination, per-block
  structural evaluation, dedupe only in trace-by-ID. Hence: hashes not
  bodies, per-message lookups, broker-only enumeration, `seq` ordering.
- **Tempo 3.x defaults** add a ≥ 30 s lag (`query_end_cutoff`) and remove
  `query_ingesters_until`; the metrics generator, dedicated columns and the
  MCP server all live in the owner release, which lab cannot sequence.
- **Retention mismatch.** LGTM 7 d against 30 d unacked in the broker: a
  thread vanishes from the view while the broker still holds it. The skill
  states "absent from the view".
- **Projection loss** past the collector (in-memory queue, single-replica
  Alloy, Tempo's 45-minute window) is measured by the reconciliation gauge,
  not prevented; the outbox covers exporter-side loss only.
- **In-cluster wiring is default-deny.** Without the NetworkPolicy grant
  and the Alloy allowlist the spans and lines are dropped silently; the
  declared-versus-live precedent is honey and sting's journal shippers,
  declared armed with zero Loki streams over 7 d (estate-lgtm lane,
  `query_loki_stats`).
- **Double counting.** A second scrape of the broker (Alloy) would make
  every `rate()` wrong; L3's `count by (job)(up)` check guards it.
- **Cardinality.** `agent_id`, `pid` or `session_id` on a long-lived series,
  or `resource_to_telemetry_conversion` on harness metrics, mints a series
  per session against Mimir's 1 M cap.
- **neo.** agentd is measured against the numeric bound proposed in
  ADR-0001 → P2 (RSS ≤ 32 MiB, CPU ≤ 36 s per hour, ≤ 10 wakeups per
  minute at steady state); agentd telemetry remains metadata only;
  every neo generation is remote-produced; lab contract tests use a named
  Honey-primary, Sting-fallback lane with source/host receipts (SWB-R39).
- **Darwin exec from external `/nix`** on neo and PZM (the node-exporter
  precedent; the TIN-4405 `/nix`-dark incident; PZM's unfinished USB HM
  activation).
- **Junie tool cap.** `tracker` at 98 leaves two slots; Linear tool growth
  would silently truncate Junie's broker reach; the provisioning
  measurement is the guard.
- **P1a is blocked** on `xoxd-ai/tinyland-infra#106`, owned by the GF/infra
  lane; lab describes and does not sequence it.
- **No-containment.** Every hook, launcher and profile added for the
  switchboard fails open; the only refusal is broker-side
  exclusive-vs-exclusive (SWB-R16).

**Facts this ADR relies on that were not verified live** (from the
critique; each is settled by the exit that names it):

- the running mcp-grafana on honey is 0.17.2 (only the pin was read);
- `tailscale serve` preserves the tailnet Host header on the loopback hop
  (lab's own registry note, not tested);
- which tailnet **nodes**, as opposed to the dollhouse user groups, the ACL
  admits to the OTLP and query proxies: neo's probes succeeded
  (`875fc21f`), honey and bumble were refused once at an operator-proxy
  port (medium confidence), and the retained Services' reachability from
  each lab host was not probed;
- the membership of `group:dollhouse-admins` and `group:dollhouse-users`;
- the tfvars value of the mail stack's `alertmanager_ntfy_topic` (read by a
  lane as `blahaj-mail-alerts`; only the variable was re-read here);
- how `swb whoami` binds to the calling Codex session (no lane evidence;
  P1b exit);
- `settingsJson.telemetry.enabled` is inert (medium confidence);
- how many tools mcp-grafana 1.6.0 enables by default (so the `ops` count
  after the bump);
- whether `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, which
  `kimi-claude.sh:265` sets, affects Claude Code OTel for Kimi;
- whether a user-domain Home Manager LaunchAgent from external `/nix` fails
  on neo the way the system LaunchDaemon did;
- whether the broker's Litestream target shares the RustFS bucket and
  failure domain (the 2026-08-23 EDQUOT outage) with the LGTM stores;
- whether containerd's CRI 16 KiB line split reaches
  `loki.source.kubernetes` reassembled;
- whether Tempo 2.7.2 evaluates the `link:` TraceQL scope;
- whether Claude Code propagates `traceparent` into **hook** processes.
  The harness-telemetry lane reads the documentation (high confidence) as
  covering Bash and PowerShell subprocesses, `-p` sessions, and "outbound
  HTTP MCP requests"; the broker, as a Streamable-HTTP MCP server, may
  therefore see a `traceparent` header on tool calls, which L5's link exit
  can use. Hooks are the undocumented case.

## Open rulings

*Amended 2026-09-26 (R-N13).* None of the nine interview questions remains
unanswered. Numeric agentd and Mimir budgets remain **proposed**, to be
confirmed or replaced before their respective drills; SWB-R45 did not
ratify those numbers. Event shapes, allowlists and proof mechanisms are
implementation work under the rulings, not new operator decisions.

For dated-history references, the September 25 list (formerly 5–13,
renumbered 1–9 after SWB-R33–SWB-R36) resolved as follows:

| Former item | Answer |
| --- | --- |
| 1: agentd telemetry | SWB-R40 allows it; SWB-R47 limits it to metadata |
| 2: Pi phase | SWB-R37: P1b, registration and threaded round trip |
| 3: yoga/mbp-13 | SWB-R38: after Neo–Sting acceptance; SWB-R45 orders the tranche |
| 4: Junie/Pi combined access | SWB-R41: P1b, defaults preserved, budgets measured; L0 prerequisite retained by SWB-R48 |
| 5: Grafana target | SWB-R42: retarget after ACL and canary proof; separately measured proxied-tool allowlist |
| 6: L4 carrier | SWB-R43: dedicated linked owner ticket TIN-5022; blahaj sequences, no date implied |
| 7: telemetry source | SWB-R44: enforced source proof; SWB-R48 puts qualification after v1 |
| 8: lab test lane | SWB-R39: named Honey-primary, Sting-fallback entrypoint, source/host receipts |
| 9: host order and budgets | SWB-R45: Honey/Bumble then yoga/mbp-13 after Neo–Sting; PZM storage gate; numbers remain proposed |

The original question wording and all earlier supersessions remain in git
history; the dated refutation table below retains its historical numbers.

## Rulings

Sources: TIN-4655 comment `73f1ce72-28c5-42d6-9039-1135e1c92121`
(2026-09-25T17:57Z, "R0: LGTM rulings interview") for SWB-R25 to SWB-R32,
and TIN-4655 comment `f870729e-dbcb-451d-97c3-0c1e1f8e9641`
(2026-09-25T19:11Z, "Open rulings on ADR-0002 / agent-switchboard #4",
operator interview) for SWB-R33 to SWB-R36, which answer this ADR's former
Open rulings 1–4. Operator questions and
asides are not rulings (R-N13). September 26 carriers are `2aeb4bee`
(SWB-R37–SWB-R39), `8a524db4` (SWB-R40–SWB-R42), `f1893e4c`
(SWB-R43–SWB-R47) and `ef8a1cd2` (SWB-R48). SWB-R02's rewording is recorded in
ADR-0001's table. The critique fixes were adopted as a block under ruling
3 and are design, not separate IDs; the table after this one lists where
each landed.

**Quote discipline.** In the table, text in quotation marks is the
operator's own pick or words exactly as the comment records them (the
answer strings such as "7d in LGTM, ACL decided before P1b (Recommended)",
and the ruling 4 prose). The sub-bullets under each numbered ruling in
that comment are the recording seat's description of what the pick means;
they are reproduced here **without quotation marks** and attributed as
"recorded as". A ruling ID's operative content is that recorded
description, adopted by the pick; it is not claimed as the operator's
wording.

| ID | Date | Operator's pick (quoted) | Recorded as (the comment's bullets, unquoted) |
| --- | --- | --- | --- |
| SWB-R25 | 2026-09-25 | R0 ruling 2: "7d in LGTM, ACL decided before P1b (Recommended)" | Tempo and Loki keep 7 days, and the broker keeps 30 days for unacked messages. |
| SWB-R26 | 2026-09-25 | same pick | Bodies go only to Loki, never into Tempo span attributes. tinyland.dev L1 adds a `loki.process` stage. (The route that stage presupposes, broker stdout → Alloy, is the plan's architecture C.) |
| SWB-R27 | 2026-09-25 | same pick | Bodies stay out of stdout until a named body-read ACL audit is decided, including who can read Loki today. The decision comes before P1b. The operator's later answer to the shared read-ACL question is on TIN-4668 (`1001c0fb`, "A: tailnet ACL, admins + MCP"); carrying it onto TIN-4655 as this ruling's decision is asked in Open rulings 1. *Addendum 2026-09-25 (`f870729e`, SWB-R33):* that TIN-4668 answer is this ruling's decision, now carried on TIN-4655; bodies ship only once ACL A is enforced live. |
| SWB-R28 | 2026-09-25 | R0 ruling 3: "Adopt, with the critique fixes (Recommended)" | The LGTM steps L0–L5 are interleaved with the broker phases. Dashboards go to tinyland.dev's owner release, and paging alerts to blahaj prometheus-mail. |
| SWB-R29 | 2026-09-25 | same pick | L0 (the `mcp-grafana` bump) starts only after a Host-header canary, since the critique flagged a likely HTTP 403 for tailnet clients. |
| SWB-R30 | 2026-09-25 | same pick | A Fable lane revises ADR-0001 and adds ADR-0002 (the LGTM plane), folding in every critique fix. Opus refutes it, then a PR goes through the fork. |
| SWB-R31 | 2026-09-25 | R0 ruling 4, the operator's words: "I'll like it all on, all being shipped. this is what can help agents opractively and efficiently pickup from others work, woithout needing to reconsruct from in agent transcripts / wal etc"; content pick: "Full content, scrubbed at the collector (Recommended)" | Prompts, tool input and output, and responses ship in full, after a collector redaction processor strips known secret shapes (tokens, keys, sops values, age and SSH keys). The body-read ACL decision gates it too. |
| SWB-R32 | 2026-09-25 | same ruling, ingest pick: "Request it from tinyland.dev/blahaj (Recommended)" | A tailnet OTLP/HTTP endpoint on the existing collector, with the scrub processor and the ACL; the unblocker for TIN-75; filed as TIN-4668. Lab renders the harness exporters gated off until that endpoint exists. This supersedes the lab default `tinyland.claudeCode.telemetry.enable = false` as the target state, but not until the endpoint and scrubber exist. The operator's later words on the same subject (TIN-4668 `1001c0fb`): lab keeps "the harness exporters gated off, and never to point them at the old `otlp-observability-http:4318`". |
| SWB-R33 | 2026-09-25 | `f870729e` item 1, SWB-R27 body gate: "Once ACL A is enforced live (Recommended)" | The operator's TIN-4668 answer, "A: tailnet ACL, admins + MCP" (`1001c0fb`), is SWB-R27's decision. `swb` bodies ship only after TIN-4670 applies ACL A in `Jesssullivan/tailnet-acl`, and a raw Loki read from a non-admin tailnet node is refused. |
| SWB-R34 | 2026-09-25 | `f870729e` item 2, `swb` bodies and the scrubber: "Redact in the loki.process stage (Recommended)" | tinyland.dev's `loki.process` stage for the broker namespace gets a redaction step with the same pattern set as the TIN-4668 collector scrubber. |
| SWB-R35 | 2026-09-25 | `f870729e` item 3, Tempo scope for harness telemetry, the operator's words: "I think tool calls, files refenced etc would be suitable for tempo, recently accessed tickets, files, paths etc in timeserice db like tempo seems like valuable  time efficient lookup for agents orienting to a complex workstream or sprint / epic / codebase." And the clarification: "indeed; we are treating tempo as a timespace efficent recency == relacence, for miniizing repeated mining calls to loki; this is using tempo as is it architected, not as it is intended." | Tempo is the recency index, where recency means relevance. Tool calls, referenced files and paths, and recently accessed tickets are span attributes. Bodies and full content (prompts, tool output) stay in Loki; that also fits Tempo's 2 KiB attribute cap. The aim is to cut repeated Loki mining when an agent orients to a workstream, sprint, epic or codebase. |
| SWB-R36 | 2026-09-25 | `f870729e` item 4, L5 timing: "Start once its three gates hold (Recommended)" | L5 is independent of P4. It starts when the TIN-4668 scrubbed endpoint, the scrubber and the enforced ACL all exist. |
| SWB-R37 | 2026-09-26 | `2aeb4bee`, item 2: "P1b with broker MVP (recommended)." | Pi's `agents` profile ships in P1b; acceptance includes registration and a threaded round trip. Preserve the existing default until explicit `agents` selection. |
| SWB-R38 | 2026-09-26 | `2aeb4bee`, item 3: "After Neo–Sting acceptance (recommended)." | yoga and mbp-13 enroll after the initial Neo Claude ↔ Sting Codex P1b round trip; each host needs delivery evidence. |
| SWB-R39 | 2026-09-26 | `2aeb4bee`, item 8: "Honey primary, Sting fallback (recommended)." | P1b contract tests use a named lab remote entrypoint targeting Honey, with Sting fallback; every receipt names exact source and execution host. Neo does no builds. |
| SWB-R40 | 2026-09-26 | `8a524db4`, item 1: "Allow agentd `swb` telemetry." | Supersedes the proposed broker-only writer constraint. Event shape and custody need bounded design; no body emission or process-control authority. |
| SWB-R41 | 2026-09-26 | `8a524db4`, item 4: "Combine during P1b." | P1b includes explicit combined broker plus LGTM read paths for Junie and Pi, preserving default groups/profiles and measuring combined tool budgets. Usable L0 reads are a functional prerequisite. |
| SWB-R42 | 2026-09-26 | `8a524db4`, item 5: "Retarget after proof (recommended)." | Honey's Grafana MCP moves to the canonical Ingress only after the precise ACL grant and live Host-header canary; proxied Tempo tools wait for a separately measured allowlist. |
| SWB-R43 | 2026-09-26 | `f1893e4c`, item 6: "Dedicated linked owner ticket (recommended)." | L4 is carried by tinyland.dev owner-release ticket TIN-5022, linked to TIN-4655 and sequenced by blahaj. No delivery date implied. |
| SWB-R44 | 2026-09-26 | `f1893e4c`, item 7: "Enforce source attribution (recommended)." | Collector proof of source identity precedes treating any line/span/series as broker or agentd output. Reject spoofed client identity; enrichment alone is insufficient; the broker remains authority for consequential state. |
| SWB-R45 | 2026-09-26 | `f1893e4c`, item 9: "Honey/Bumble, then yoga/mbp-13 (recommended)." | Follows Neo Claude ↔ Sting Codex P1b acceptance. PZM stays behind storage delivery. Numeric agentd and metric budgets remain proposals. |
| SWB-R46 | 2026-09-26 | `f1893e4c`, duplicate clarification: "Reject with opaque conflict (recommended)." | A repeated global `msg_id` with different sender or body is rejected without another message's receipt or metadata. Exact retries remain idempotent. |
| SWB-R47 | 2026-09-26 | `f1893e4c`, telemetry shape: "Metadata only: lifecycle, counts, errors (recommended)." | Agentd `swb` telemetry carries no bodies or other content. SWB-R33/SWB-R34 continue to gate any separate body path. |
| SWB-R48 | 2026-09-26 | `ef8a1cd2`: "Only telemetry proof follows v1 (recommended)." | P1b still includes the combined Junie/Pi broker-plus-LGTM path, usable L0 prerequisite and measured tool budgets. Collector scrubbing and enforced telemetry provenance are post-v1 qualification and do not block the broker MVP. Bodies stay disabled until SWB-R33 live ACL proof and SWB-R34 redaction qualification. |

**Critique fixes carried (R0 ruling 3, "Critique fixes the ADR revision
must carry"):**

| Fix, as listed in `73f1ce72` | Where it landed |
| --- | --- |
| "The switchboard Claude hooks merge at `mkDefault`, so the three fail-closed guard hooks survive, with a contract test." | Enrollment; ADR-0001 P1b |
| "Claude and Kimi enrollment goes through `vars/mcp_registry.yml` → export-registries → `tinyland.mcp` → `~/.claude.json`." | Enrollment; ADR-0001 Harness reach |
| "Codex `notify` is runtime-owned, so there is no new table." | Enrollment; ADR-0001 Identity, Lease, P4 |
| "The Junie/Pi lookup gap is stated: one Junie server, single-profile Pi." | Lookup recipes; Enrollment; ADR-0001 Harness reach; Open rulings 4 (numbered 8 before the 2026-09-25 amendment) |
| "The Loki schema needs a `loki.process` stage." | Data model → Loki (SWB-R26); L1 |
| "The projection is lossy, so it needs a transactional outbox or a reconciliation gauge, and 'absent' means expired or not projected." | Outbox and reconciliation; principle 7; L2 exit |
| "Each lifecycle step gets its own trace, linked back to the send span." | Data model → Identifiers |
| "Thread enumeration stays broker-only." | Lookup recipes → TraceQL and LogQL (both thread recipes are marked sample-only with a completeness check against `thread`'s max `seq`); AGENTS.md reader rule |
| "tinyland.dev citations are re-pinned to current main." | Every tinyland.dev citation is at `68a16f65` |

Other critique problems that held and were folded in: the L0 403 risk and
Host strategy (SWB-R29 and L0); the unverified `ops` budget (L0); bodies
readable before the ACL gate, with `group:dollhouse-users` named (SWB-R27,
L1, Risks); egress through cross-thread reads (Risks; SWB-R33, formerly Open rulings 1);
unobservable exit checks replaced (P1b, L1, L2, L3); the L3 alert data
path through federation (L3); `agent_id` cardinality (Mimir series);
agentd from external `/nix` (Enrollment, ADR-0001 P2); the deferred false
artifact (Enrollment, ADR-0001 P1b); `diff_tempo_traces` on 2.7.2, the
vParquet column limits, the textfile directory owner, the TIN-75 gate's
real scope, and the auto-memory ruling being about durability rather than
privacy (corrected in place); broker-derived trace ids and
`delivery_count` in span ids (Identifiers); yoga and tailnet-acl ownership
(Ownership table, Open rulings 3, formerly 7); R0's durable carrier (the agent note
named in ADR-0001 → Phases).

Estate rulings this design depends on, beyond ADR-0001's list:

- **Alerting ownership** (lab `AGENTS.md`, 2026-08-29): prometheus-mail
  rules plus the mail Alertmanager are the estate alerting surface; lab
  authors no alert rules from its seat.
- **Verify against current `origin/main`** (lab `AGENTS.md`, 2026-08-29):
  the reason every citation here is re-pinned, and why the peer-dialog
  skill's sha-locked manifest is re-rendered against current main when it
  lands.
- **Remove false artifacts as you go** (lab `AGENTS.md`, 2026-07-22): the
  `settingsJson.telemetry.enabled` disposition.
- **Junie dials one server; profiles replace the default** (lab `AGENTS.md`,
  TIN-4415, 2026-09-19) and **Pi's ratified `zero` profile** (TIN-3017):
  the source of the lookup gap.
- **Codex `features` is runtime-owned** (lab `AGENTS.md`): "declaring it
  would mean owning it", the precedent applied to `notify`.
- **Harnesses run unconstrained** (lab `AGENTS.md`, 2026-08-31): every
  hook, launcher and profile this design adds fails open, and nothing in
  it refuses a harness a flag, a directory or a launch. The read-ACL
  narrowing on Loki and Tempo is the operator's own decision (`1001c0fb`,
  TIN-4670), not a lab-authored layer.
- **Landing is not delivering**, **neo is the teletype machine** with the
  local-build guard, and **PZM's USB external `/nix`** posture (lab
  `AGENTS.md`): the per-host delivery and verification rules above.
- **The one-LGTM-release contract** (`blahaj@2071613c
  config/observability-authority.json`): no second collector, no second
  LGTM MCP, 7-day retention.

## Refutation review (2026-09-25)

*Historical record, clarified 2026-09-26:* the table below describes the
September 25 review, not current open decisions. SWB-R37–SWB-R48 resolve
the later interview: in particular P1b now consumes L0 for Junie/Pi,
agentd metadata telemetry is allowed, Pi placement is ruled, and opaque
conflicts replace the proposed stored-receipt response. See current
[Open rulings](#open-rulings) for the resolved mapping and remaining
proposed numeric budgets.

SWB-R30: "Opus refutes it, then a PR goes through the fork." The Opus
refutation of the first draft returned "ship-with-fixes" with 23 problems
and 7 ruling-fidelity findings. Every one held and is applied; none is
rejected. Where a fix needed an operator decision, the ADR asks for it in
[Open rulings](#open-rulings) instead of deciding.

*Note 2026-09-25 (amendment):* this table and the paragraph after it are
the record of the refutation as handled at `16110709`, and keep that
commit's Open-rulings numbers. Since then items 1–4 were ruled (1 →
SWB-R33, 2 → SWB-R35, 3 → SWB-R36, 4 → SWB-R34) and items 5–13 are
renumbered 1–9 (5 → 1, 6 → 2, 7 → 3, 8 → 4, 12 → 8, 13 → 9).

| # | Problem, in short | Handling |
| --- | --- | --- |
| 1 | Stale estate facts: the tailnet already exposes the collector's OTLP 4317/4318, unscrubbed; Loki and Tempo answer without credentials; `26a311db`, `875fc21f`, `1001c0fb`, TIN-4670 uncited | Context "What runs today", Auth, Risks and the unverified list rewritten against `26a311db` and `875fc21f`/`1001c0fb`; blahaj `:58-102` and registry `:279-282` cited; TIN-4670 named; the ingest-path paragraph no longer claims hosts cannot reach a proxy |
| 2 | The L5 gate "until that endpoint exists" could be met by the wrong 4318 | L5 names `otlp-harness-http-tailscale` (4318 → 14318, `otlp/tailnet`), forbids `otlp-observability-{http,grpc}` with the operator's words, adds a rendered-endpoint gate and exit, and records TIN-4670's retire-or-repoint as the operator's later pick with the bypass stated meanwhile; the ADR-0001 delta row said the same (*that row was deleted 2026-10-04 with ADR-0001's Deltas tables, R-C275*) |
| 3 | SWB-R27's status is stale: the operator answered "A: tailnet ACL, admins + MCP" on TIN-4668; "decided" is not "enforced" | Auth, L1 and the rulings row cite `1001c0fb`; Open rulings 1 asks for the TIN-4655 carrier and for the decision-versus-enforcement gate; the "needs its own interview under the no-containment ruling" framing is removed everywhere |
| 4 | The L5 content exit cannot pass in Tempo (2048-byte cap; the build's `traces/tailnet` allowlist) | Loki through `logs/tailnet` is the L5 content store; the content exit is a Loki check; scrub checks cover both signals; whether ruling 2's Tempo ban covers harness content is Open rulings 2 |
| 5 | Drills need someone to stop or scale a process; R-N11 makes that operator-only | The phases intro states it once; the L0 canary, the L2 restart and reconciliation drills, the L3 scale-to-zero and ADR-0001's P1b pod-restart and broker-down drills each name the operator as the actor |
| 6 | The LogQL thread recipe enumerates a thread through Loki and can truncate silently | Both LogQL thread recipes are sample-only, need an explicit `limit`, and require a completeness check against `thread`'s max `seq`; the critique-fix table and AGENTS.md say the same |
| 7 | Reconciliation counted search results; nothing grants the broker Tempo/Loki reads | Reconciliation is a set difference over distinct `msg_id`s on half-open 5-minute slices with saturation detection; the Tempo 3200 / Loki 3100 grant is an L2 owner item and exit |
| 8 | `just remote-check` is this repo's recipe; lab has none | Enrollment states the lab evidence (`git grep`, `justfile:789-846`) and the by-hand lane; Open rulings 12 records the missing lane with an owner and a deadline |
| 9 | The Codex identity binding is unspecified and unevidenced | Marked unproven: the binding must be an env var or path Codex exposes, never pid ancestry; ADR-0001 → Identity and the P1b exit make Codex registration its own observable sub-exit |
| 10 | The L2 dedupe exit was vacuous (trace-by-ID always dedupes) | Replaced by a set-difference check plus a recorded raw search and tag-values count where a resend can appear |
| 11 | Numeric bounds were deferred | Proposed numbers are in the ADR (neo agentd RSS ≤ 32 MiB, CPU ≤ 36 s/h, ≤ 10 wakeups/min, ADR-0001 → P2; ≤ 10,000 harness series over 7 days, L5), owner lab, confirmed or replaced in each drill's plan comment; Open rulings 13 |
| 12 | The L1 audit scope missed readers | Adds `tempo-observability:3200`, `group:dollhouse-admins`, the `tag:k8s-operator` grant, every Grafana user and the MCP proxy's service account |
| 13 | `swb` bodies bypass every scrubber | Stated in the Loki schema and Risks; Open rulings 4 asks whether SWB-R19 accepts it or an Alloy-side stage is added; not decided here |
| 14 | `swb.body_sha256` lets a reader confirm a guessed short body | Both signals carry a keyed `body_hmac` under a broker-held sops key; the residual `size` exposure is stated in Risks |
| 15 | "A client cannot pick a trace id" overstated, since `msg_id` idempotency was unscoped | `msg_id` is globally unique at the broker, with the retry and conflict semantics stated in Identifiers and in ADR-0001 → Mailbox |
| 16 | The two ADRs contradicted each other (L0 dependency, the outbox's phase, the P4 heading, the phase orders, principle 3) | L0 is after P1a in sequence with the canary as its only gate; the outbox and projector land in L2 and P1b writes stdout after commit; P4 is "Codex push" again and Pi stays in P1b as proposed (Open rulings 6); the phase order, SWB-R27 step included, is carried by ADR-0001 → Phases (*updated 2026-10-04, R-C275:* AGENTS.md and the README no longer restate it, after agent-switchboard #14 and #12); principle 3 is scoped to `swb` telemetry |
| 17 | An ingress rule into the broker's namespace was assigned to tinyland.dev | Assigned to blahaj in L3 |
| 18 | "Accepted (design)" before the refutation and the PR | Proposed until the PR merges; unruled design choices are named as proposals in the status line |
| 19 | Unpinned or missing citations; TIN-75 cited by its status field | Tempo, OTel, Prometheus and Loki docs pinned to `main` shas re-read on 2026-09-25; Grafana 12.3.6+security-04, prometheus-mail v2.51.0, Mimir 2.17.10, `GF_LIVE_MAX_CONNECTIONS=0` and the ntfy topic cited to lane tool calls and file:line; the textfile owner cited to blahaj `honey_hardware/tasks/main.yml:131-138` and the PZM path to lab `petting-zoo-mini.nix:619,638`; TIN-75 cited by `stateHistory` |
| 20 | The `traceparent` item contradicted the lane (outbound HTTP MCP requests are covered) | Corrected to match the lane; hooks are the only undocumented case |
| 21 | The ops budget fix was only partly carried | The `ops` count is measured on the L0 canary before the honey switch, and the switch is blocked on `status.json` passing |
| 22 | The agent note overclaimed, and the notes README pointed only at ADR-0001 | The note's summary is softened and gains a refutation section; the README points at both ADRs |

Ruling-fidelity findings: agentd-never-emits-OTLP is no longer listed as
ruled (AGENTS.md, ADR-0001's SWB-R17 row; Open rulings 5); the
no-containment ruling is no longer cited to make the read ACL an interview
item; L5's gates are the three ruling 4 names and its P4 placement is Open
rulings 3; `OTEL_LOG_RAW_API_BODIES` is in the "all on" set; the Pi profile
is placed once (P1b, proposed; Open rulings 6); AGENTS.md quotes SWB-R02
verbatim; the rulings table separates the operator's quoted picks from the
recorder's bullets.
