# swb reference

Settings, routes, tools and verbs for `swb` on `main`: v0.3.0, not yet
released. v0.3.0 breaks the v0.2 wire format; what changed from v0.2.2 and
the upgrade order are in
[releases/v0.3.0-migration.md](releases/v0.3.0-migration.md). The design and
its rulings are in [ADR-0001](adr/0001-agent-switchboard.md); a walkthrough
is the [README quickstart](../README.md#quickstart); harness configuration is
in [integrations/](../integrations/).

## Agent identity

An agent id is `harness:host:pid:session_id`, for example
`claude:sting:40001:sess-a`. `register` (and the SessionStart hook) returns
it, together with an end token (v0.3.0) that only the registering session
receives. Sessions are `live` for 15 minutes after their last call, `idle` up
to 6 hours, then `gone`. Messages are kept 7 days after acknowledgement and
30 days unacknowledged; a message expires after 72 hours by default (14 days
at most). The broker prunes expired rows at startup and every hour.

Identity is self-asserted: the end token stops a caller ending a session it
did not register, but anyone who can register can re-register any agent id
(see [SECURITY.md](../SECURITY.md)).

## Settings

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

## REST routes

All on `SWB_LISTEN`, behind the Host and Origin guard. Every state change is
a POST with a JSON body (`Content-Type: application/json`).

| Route | Body or query | Notes |
| --- | --- | --- |
| `POST /v1/register` | `{harness, host, pid, session_id, proc_start}` | returns `{agent_id, lease_seconds, end_token}` |
| `POST /v1/end` | `{me, end_token}` | any other token answers `{"state":"unknown"}` and ends nothing |
| `POST /v1/peers` | `{me?, all?, host?, limit?, cursor?, proc_start?}` | `me` renews that session's lease |
| `GET /v1/peers` | `?all=&host=&limit=&cursor=` | passive listing; changes nothing |
| `POST /v1/send` | `{from, to, ticket, body, …}` (envelope v3) | `ticket` is `TIN-<digits>` or `none`; a body is at most 16 KiB |
| `POST /v1/inbox` | `{me, limit, wait_seconds}` | long-poll |
| `POST /v1/ack` | `{me, msg_id}` | |
| `GET /healthz` | | also on `SWB_METRICS_LISTEN`, where it is not Host-guarded |

`peers` lists live sessions by default (`all` adds idle, gone and ended),
one row per harness, host and session id, in pages of `limit` (default 50,
at most 200) with a `next_cursor`. A row is
`{agent_id, harness, host, last_seen, state}`. `GET /v1/peers?me=` and
`GET /v1/inbox` answer 405.

Errors are `{"error", "code"}` with a status per kind: 400
(`invalid_argument`, `invalid_json`), 404 (`unregistered`,
`message_unavailable`, `not_found`), 405 (`method_not_allowed`), 409
(`msg_id_conflict`), 415 (`unsupported_media_type`), 500 (`internal`). The
guard's refusals are 403. The full table is in the migration notes.

`/metrics` is on `SWB_METRICS_LISTEN`.

## MCP tools

At `/mcp`: `register`, `end`, `peers`, `send`, `inbox`, `ack`, with the same
fields and rules as the REST routes; every input field is described in the
tool schema. Tool errors carry the same JSON error object as their text. Over
MCP the `register` result carries `end_token` into the model's context; an
adapter that ends sessions itself should keep the token and strip it.

## CLI verbs

`serve`, `hook <harness> <event>`, `whoami [--register]` (plain `whoami`
makes no broker call), `inbox`,
`send <to> <ticket> [--ruling ID] [--operator-directed] [--in-reply-to ULID]
[--thread ULID] [--msg-id ULID] < body`, `ack <msg_id>`,
`peers [--all] [--host H] [--limit N] [--cursor ID]`, `doctor [--register]`,
`channel` and `version`.

- `--help`, `-h`, `help` and `<verb> --help` print usage on stdout and exit
  0; `--version`, `-V` and `version` exit 0. An unknown verb exits 2.
- `peers` never passes `me`, so it refreshes no lease.
- Hooks save the end token on every successful register, under
  `$SWB_STATE_DIR/sessions/`; SessionEnd spends it and deletes the file.
  The CLI never prints the token: `whoami --register` prints
  `end_token_file` in its place.
- `agentd` is planned for P2 and exits 3; it is not in the usage text.

Exit codes: 0 ok, 2 usage, 3 missing setting, broker error or a failed
`doctor` check, 1 `serve` failure; `hook` always exits 0, and `channel`
exits 0 when Claude Code closes its stdin.

## Claude Code channel

`swb channel` is a stdio MCP server that declares the `claude/channel`
capability (ADR-0001 → Push, v1c). Register it as a plain MCP server, for
example under the name `swb-channel`, with the command `swb` and the
argument `channel`; the [Claude Code plugin](../integrations/claude-code/)
does this. It reads the session's `SWB_*` environment, so the session must
start with the identity variables set (`SWB_PROC_START` included, which the
channel uses to find its own row; in the home estate a lab launcher exports
them, R-C273). During Claude Code's channels research preview a custom
server loads only with
`claude --dangerously-load-development-channels server:swb-channel`, which
shows a confirmation at startup. Events arrive as
`<channel source="swb-channel" from="…" ticket="…" msg_id="…" authority="peer">`.
Answer with the `reply` tool or acknowledge with `ack`; the channel never
acknowledges on its own.

## Notices

On UserPromptSubmit, `swb hook <harness> UserPromptSubmit` adds a short
notice to the session's context when it has unread mail. The notice names the
receiver's agent id, the sender and ticket, and the exact `swb inbox` and
`swb ack` commands.

## Known gaps

- Session rows are never pruned.
- `/v1/notify` is not implemented.
- `agentd` (per-host push) and the claim, release and handoff tools are
  planned for P2.
