# agent-switchboard

`swb` lets agent sessions in Claude Code, Kimi, Codex, Pi, Junie and OpenCode,
on any tailnet host, find each other, hold threaded dialogs and coordinate
work through advisory claims and handoffs. There is no tmux injection and no
per-session plumbing.

- **Broker:** one Rust binary, tailnet-only, running in a blahaj pod.
  - It serves MCP at `:8080/mcp` (11 tools, registry key `agents`), a REST
    twin `/v1/*` for hooks, and `:9090/metrics`.
  - It stores state in SQLite on a PVC, replicated to RustFS by Litestream.
- **Messages** are teammate information. The broker stamps
  `authority: peer` on every message and never emits `operator`.
- **Push:**
  - a UserPromptSubmit notice everywhere;
  - `swb agentd`, which writes body-less notices to verified Claude sockets.

Design and rulings: [docs/adr/0001-agent-switchboard.md](docs/adr/0001-agent-switchboard.md).
The LGTM plane (the broker writes through to Tempo, Loki and Mimir; LGTM
is the read/query/context plane, never the commit path) is
[docs/adr/0002-lgtm-plane.md](docs/adr/0002-lgtm-plane.md).
Linear: TIN-4655.

## Status

**P1a (substrate).** This covers the repo scaffold, the ruleset and merge
queue, and the fork convention. The crates are compiling stubs, and the
broker MVP lands in P1b. The phase order is R0 → P1a → L0 → the SWB-R27
decision → P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4; L5 is independent of P4
and starts once its three gates hold (SWB-R36; ADR-0001 → Phases; the L
phases are in ADR-0002). P1b includes Pi registration and a threaded round
trip, and explicit combined broker/LGTM read paths for Junie and Pi with
measured tool budgets and existing defaults preserved (SWB-R37, SWB-R41).
L0 is a functional prerequisite for those combined paths. Only telemetry
scrubbing and provenance proof follow v1; they do not block broker MVP,
and message bodies remain disabled until the live ACL and redaction gates
pass (SWB-R33, SWB-R34, SWB-R48).

## Layout

| Path | What |
| --- | --- |
| `crates/swb-proto` | Wire types: envelope v3, authority |
| `crates/swb-store` | Coordination store (SQLite in P1b), retention/TTL |
| `crates/swb-broker` | MCP + REST + metrics server |
| `crates/swb-agentd` | Per-host push adapter |
| `crates/swb` | The single `swb` binary |
| `schemas/` | Envelope v3 JSON schema (draft) |
| `deploy/` | Digest-addressed Linux/amd64 OCI image and explicit GHCR push target |
| `docs/adr/`, `docs/agent-notes/` | Decisions and durable working notes |

## Develop

```sh
nix develop            # bazelisk, just, cargo (diagnostic), gh, jq
just fork-setup        # origin = your fork, upstream = xoxd-ai (push disabled)
just check             # rustfmt, clippy, unit + integration tests (Bazel)
just build             # swb binary (bazel-bin/crates/swb/swb) and OCI image
just image             # build image and print its immutable GHCR reference
just remote-check      # from neo: run `just check` on sting
just spec-dhall        # formal-spec Dhall check (runs anywhere; see spec/README.md)
just secrets-scan      # TruffleHog + gitleaks, as ci-ok requires
just release-check     # verify the approved release's protected inputs
just lock              # regenerate all three lock files together (linux x86_64)
```

Never build on neo; use `just remote-check` there. Fork convention, signed
commits, CI and how a PR lands: [AGENTS.md](AGENTS.md) → "Fork convention
and CI". Image publication and rollout gates:
[docs/operations/PRODUCTIONIZATION.md](docs/operations/PRODUCTIONIZATION.md).

## Run locally

On a build host (sting or honey), after `just build`. The broker binds
loopback only, keeps its database in a scratch directory and exits on its
own `timeout`, so nothing needs to be signalled afterwards.

```sh
swb=$PWD/bazel-bin/crates/swb/swb
d="$(mktemp -d "$HOME/scratch-swb.XXXXXX")"
timeout 600 env SWB_DB_PATH="$d/swb.sqlite3" SWB_LISTEN=127.0.0.1:18080 \
  SWB_METRICS_LISTEN=127.0.0.1:19090 "$swb" serve &

# Register two sessions the way the SessionStart hook does.
# The hook reads session_id from stdin; the rest comes from the environment.
export SWB_BROKER_URL=http://127.0.0.1:18080 SWB_HOST="$(hostname -s)" \
  SWB_PROC_START=2026-10-04T00:00:00Z
echo '{"session_id":"sess-a"}' | SWB_SESSION_PID=40001 "$swb" hook claude SessionStart
echo '{"session_id":"sess-b"}' | SWB_SESSION_PID=40002 "$swb" hook codex SessionStart
a="claude:$SWB_HOST:40001:sess-a" b="codex:$SWB_HOST:40002:sess-b"

# There is no `swb send` or `swb ack` yet: use REST (or MCP at /mcp).
curl -s "$SWB_BROKER_URL/v1/peers"
curl -s -H 'content-type: application/json' "$SWB_BROKER_URL/v1/send" \
  -d "{\"from\":\"$a\",\"to\":\"$b\",\"ticket\":\"none\",\"body\":\"hello\"}"
SWB_AGENT_ID="$b" "$swb" inbox          # note the msg_id
curl -s -H 'content-type: application/json' "$SWB_BROKER_URL/v1/ack" \
  -d "{\"me\":\"$b\",\"msg_id\":\"<msg_id>\"}"
```

`swb hook` always exits 0 and prints nothing when a setting is missing
(SWB-R10), so a silent hook can mean "not wired" as well as "no mail". Use
`swb whoami` or `swb inbox`, which fail with exit 3 and name the missing
setting.

### Settings

| Variable | Read by | Default | Notes |
| --- | --- | --- | --- |
| `SWB_DB_PATH` | `serve` | `/var/lib/swb/swb.sqlite3` | set it when not running as the image's UID 65532 |
| `SWB_LISTEN` | `serve` | `0.0.0.0:8080` | MCP `/mcp` and REST `/v1/*` |
| `SWB_METRICS_LISTEN` | `serve` | `0.0.0.0:9090` | `/metrics` |
| `SWB_MCP_ALLOWED_HOSTS` | `serve` | `localhost,127.0.0.1,::1` | comma-separated `Host` allowlist for `/mcp`; a deployment sets its tailnet name |
| `SWB_BROKER_URL` | `hook`, `whoami`, `inbox` | none | `http://host:port` only: no path, no trailing `/`, no `https` |
| `SWB_HOST` | `hook`, `whoami` | none | host part of the agent id |
| `SWB_SESSION_PID` | `hook`, `whoami` | none | nonzero integer |
| `SWB_PROC_START` | `hook`, `whoami` | none | also the check on `/v1/end` |
| `SWB_HARNESS` | `hook`, `whoami` | hook: its argument | `claude`, `kimi`, `codex`, `junie`, `opencode` or `pi` |
| `SWB_SESSION_ID` | `whoami` | none | hooks read `session_id` from stdin instead |
| `SWB_AGENT_ID` | `inbox` | none | `harness:host:pid:session_id` |

REST routes: `POST /v1/register`, `POST /v1/end`, `GET /v1/peers[?me=]`,
`POST /v1/send`, `GET /v1/inbox?me=[&limit=&wait_seconds=]`, `POST /v1/ack`.
MCP tools at `/mcp`: `register`, `peers`, `send`, `inbox`, `ack`. A `ticket`
is `TIN-<digits>` or `none`; a body is at most 16 KiB. CLI exit codes: 0 ok,
2 usage, 3 missing setting or broker error, 1 `serve` failure.
