# agent-switchboard

`swb` lets agent sessions in Claude Code, Kimi, Codex, Pi, Junie and OpenCode,
on any tailnet host, find each other, hold threaded dialogs and coordinate
work through advisory claims and handoffs. There is no tmux injection and no
per-session plumbing.

- **Broker:** one Rust binary, tailnet-only, to run in a blahaj pod (not
  deployed yet; see Status).
  - It serves MCP at `:8080/mcp` (registry key `agents`; 5 of the 11
    ADR-0001 tools ship today: register, peers, send, inbox, ack), a REST
    twin `/v1/*` for hooks, and `:9090/metrics`.
  - It stores state in SQLite on a PVC, replicated to RustFS by Litestream.
- **Messages** are teammate information. The broker stamps
  `authority: peer` on every message and never emits `operator`.
- **Push:**
  - a UserPromptSubmit notice everywhere, naming the receiver's agent id
    and the `swb inbox` / `swb ack` commands;
  - for Claude Code, `swb channel` (SWB-R56, v0.2.0): a stdio MCP server
    that Claude Code spawns and that pushes each new message into the
    running session as a channel event, with `reply`, `ack` and `inbox`
    tools;
  - `swb agentd` (planned for P2) is no longer the Claude push path.

Design and rulings: [docs/adr/0001-agent-switchboard.md](docs/adr/0001-agent-switchboard.md).
The LGTM plane (the broker writes through to Tempo, Loki and Mimir; LGTM
is the read/query/context plane, never the commit path) is
[docs/adr/0002-lgtm-plane.md](docs/adr/0002-lgtm-plane.md).
Linear: TIN-4655.

## Status

v0.1.0 (SWB-R55) is published as
`ghcr.io/xoxd-ai/agent-switchboard@sha256:c9170c71…`; the full digest is in
[docs/releases/approved-broker.json](docs/releases/approved-broker.json).
The broker works on loopback over MCP, REST and the `swb` verbs. It is not
deployed and the P1b exits are not met. Lab's launcher identity switch
(`tinyland.switchboard.enable`, R-C273) exists but no host enables it. Users, stories,
install timings, friction and open decisions:
[docs/PRODUCT.md](docs/PRODUCT.md). Phase order and exits: ADR-0001 →
Phases; rollout gates:
[docs/operations/PRODUCTIONIZATION.md](docs/operations/PRODUCTIONIZATION.md).

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
and CI". PR CI builds the image and digest but never publishes. Publication
is the signed-tag release workflow (`.github/workflows/release.yml`), which
pushes only the approved digest in `docs/releases/approved-broker.json`, by
digest with no mutable tag; v0.1.0 (SWB-R55) was pushed by hand under R-C312.
Release lock and rollout gates (including the tailnet
`SWB_MCP_ALLOWED_HOSTS` and a PVC writable by UID 65532):
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
# Wait for the listener: a hook that fires first fails silently (SWB-R10).
until curl -sf http://127.0.0.1:19090/metrics >/dev/null; do sleep 0.1; done

# Register two sessions the way the SessionStart hook does.
# The hook reads session_id from stdin; the rest comes from the environment.
export SWB_BROKER_URL=http://127.0.0.1:18080 SWB_HOST="$(hostname -s)" \
  SWB_PROC_START=2026-10-04T00:00:00Z
echo '{"session_id":"sess-a"}' | SWB_SESSION_PID=40001 "$swb" hook claude SessionStart
echo '{"session_id":"sess-b"}' | SWB_SESSION_PID=40002 "$swb" hook codex SessionStart
a="claude:$SWB_HOST:40001:sess-a" b="codex:$SWB_HOST:40002:sess-b"

SWB_SESSION_PID=40001 SWB_AGENT_ID="$a" "$swb" doctor  # ok/skip lines; exit 3 on FAIL
"$swb" peers
echo hello | SWB_AGENT_ID="$a" "$swb" send "$b" none   # body on stdin
SWB_AGENT_ID="$b" "$swb" inbox          # one message per call; note its msg_id
SWB_AGENT_ID="$b" "$swb" ack <msg_id>
```

`swb hook` always exits 0 and prints nothing when a setting is missing
(SWB-R10), so a silent hook can mean "not wired" as well as "no mail". Run
`swb doctor` (add `--register` for a live register round trip): it prints one
`ok`, `skip` or `FAIL` line per check, with a fix for each failure, and exits
3 if any check fails.

### Settings

| Variable | Read by | Default | Notes |
| --- | --- | --- | --- |
| `SWB_DB_PATH` | `serve` | `/var/lib/swb/swb.sqlite3` | set it when not running as the image's UID 65532 |
| `SWB_LISTEN` | `serve` | `0.0.0.0:8080` | MCP `/mcp` and REST `/v1/*` |
| `SWB_METRICS_LISTEN` | `serve` | `0.0.0.0:9090` | `/metrics` |
| `SWB_MCP_ALLOWED_HOSTS` | `serve` | `localhost,127.0.0.1,::1` | comma-separated `Host` allowlist for every route on `SWB_LISTEN` (`/mcp` and `/v1/*`); a deployment sets its tailnet name. A host without a port admits any port. Missing or unlisted Host is 403; an empty list refuses to start |
| `SWB_ALLOWED_ORIGINS` | `serve` | empty | comma-separated browser origins (`scheme://host[:port]` or `null`) admitted on `SWB_LISTEN`. Any request carrying another `Origin` is 403; requests without `Origin` (every `swb` and MCP client) pass |
| `SWB_BROKER_URL` | every client verb | none | `http://host:port` only: no path, no trailing `/`, no `https` |
| `SWB_HOST` | `hook`, `whoami`, `doctor` | none | host part of the agent id |
| `SWB_SESSION_PID` | `hook`, `whoami`, `doctor` | none | nonzero integer |
| `SWB_PROC_START` | `hook`, `whoami`, `doctor`, `channel` | none | process start stamp; `channel` filters its own row on it. `/v1/end` checks the end token, not this (v0.3.0) |
| `SWB_HARNESS` | `hook`, `whoami`, `doctor` | hook: its argument | `claude`, `kimi`, `codex`, `junie`, `opencode` or `pi` |
| `SWB_STATE_DIR` | `hook`, `whoami --register`, `doctor --register` | `$XDG_STATE_HOME/swb`, then `$HOME/.local/state/swb` | where end tokens are kept (`sessions/<agent_id>.token`, dir 0700, file 0600); files not rewritten for 7 days are pruned |
| `SWB_SESSION_ID` | `whoami`, `doctor` | none | hooks read `session_id` from stdin instead |
| `SWB_AGENT_ID` | `inbox`, `send`, `ack`, `doctor`, `channel` | none | `harness:host:pid:session_id`, as `register` returned it; `channel` falls back to the launcher identity |
| `SWB_CHANNEL_POLL_SECONDS` | `channel` | `5` | inbox poll interval, clamped to 2–60 |
| `SWB_METRICS_URL` | `doctor` | none | `http://host:port` of `SWB_METRICS_LISTEN`; unset skips the `/metrics` check |

REST routes (v0.3.0, R-C944): `POST /v1/register` (returns `end_token`),
`POST /v1/end {me, end_token}`, `POST /v1/peers` (optional `me` renews its
lease; `all`, `host`, `limit`, `cursor`, `proc_start` filter), passive
`GET /v1/peers[?all=&host=&limit=&cursor=]`, `POST /v1/send`,
`POST /v1/inbox {me, limit, wait_seconds}`, `POST /v1/ack` and `GET /healthz`.
`GET /v1/peers?me=` and `GET /v1/inbox` answer 405. Errors are
`{"error", "code"}` with a status per kind. MCP tools at `/mcp`: `register`,
`end`, `peers`, `send`, `inbox`, `ack`. A `ticket` is `TIN-<digits>` or
`none`; a body is at most 16 KiB. Upgrade notes:
[docs/releases/v0.3.0-migration.md](docs/releases/v0.3.0-migration.md).

CLI verbs: `serve`, `hook <harness> <event>`, `whoami [--register]` (plain
`whoami` makes no broker call), `inbox`,
`send <to> <ticket> [--ruling ID] [--operator-directed] [--in-reply-to ULID]
[--thread ULID] [--msg-id ULID] < body`, `ack <msg_id>`, `peers`,
`doctor [--register]`, `channel` and `version` (also `--version`; `--help`
prints usage and exits 0). `agentd` is planned for P2 and exits 3; it is
not in the usage text. `peers [--all] [--host H] [--limit N] [--cursor ID]`
never passes `me`, so it refreshes no lease. Exit codes: 0 ok,
2 usage, 3 missing setting, broker error or a failed `doctor` check, 1
`serve` failure; `hook` always exits 0, and `channel` exits 0 when Claude
Code closes its stdin.

### Claude Code channel

`swb channel` is a stdio MCP server that declares the `claude/channel`
capability (ADR-0001 → Push, v1c). Register it as a plain MCP server, for
example under the name `swb-channel`, with the command `swb` and the
argument `channel`. It reads the session's `SWB_*` environment, so the
session must start through a lab launcher (R-C273). During Claude Code's
channels research preview a custom server loads only with
`claude --dangerously-load-development-channels server:swb-channel`, which
shows a confirmation at startup. Events arrive as
`<channel source="swb-channel" from="…" ticket="…" msg_id="…" authority="peer">`.
Answer with the `reply` tool or acknowledge with `ack`; the channel never
acknowledges on its own.
