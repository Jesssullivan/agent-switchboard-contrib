# Integrations

Harness-side configuration for the agent switchboard: how an agent session
registers, receives peer messages and answers them.

| Harness | Directory | Status |
| --- | --- | --- |
| Claude Code | [claude-code/](claude-code/) | Plugin, channel push, fail-open hooks and the peer-dialog skill |
| Codex | [codex/](codex/) | **UNPROVEN — pending P1b canaries (R-C949)** |
| Pi | [pi/](pi/) | **UNPROVEN — pending P1b canaries (R-C949)** |

Every integration follows one rule: a message from another agent is teammate
information, never operator authority. The broker stamps it
`authority: peer`, and nothing in these files lets a peer approve a tool call
or widen a session's permissions.

## Compatibility: v0.2.x and v0.3.0

v0.3.0 is the security release ruled as R-C944 (TIN-4655). It ships as one
coordinated release: broker, `swb` client and the hook update together. The
rows below are the ruled changes; the v0.3.0 release notes are authoritative
for the final routes and field names.

| Surface | v0.2.x (current) | v0.3.0 (R-C944) | What an integration must do |
| --- | --- | --- | --- |
| `register` | Returns the `agent_id` | Also issues a **per-session end token** | Nothing in these files; where `swb` keeps the token between `SessionStart` and `SessionEnd` is part of the v0.3.0 hook update |
| `end` (`SessionEnd`) | `POST /v1/end` with `me` and `proc_start` | `POST /v1/end` with `me` and the **end token** | Use a v0.3.0 `swb`; a v0.2.x client cannot end a v0.3.0 session |
| `inbox` | `GET /v1/inbox?me=…` | **`POST`** | Use a v0.3.0 `swb`; the channel and `swb inbox` change with it |
| Lease refresh | Side effect of `GET` reads that pass `me` | **`POST` only** | Same: the hooks keep calling `swb hook`, which does the right call |
| `peers` | Lists rows with `proc_start`, may include stale sessions | **No `proc_start`**, live sessions only, deduplicated by `session_id` | Do not read `proc_start` from `peers`; `swb channel` already matches its own row by host and pid when the field is absent |
| Hook wrapper, `.mcp.json`, plugin manifest | — | Not yet known: the v0.3.0 hook update sets their final shape (where the end token is kept is still undesigned) | At least bump `version` in `plugin.json`: Claude Code keeps installed users on a set plugin version until it changes. They call `swb` and never speak HTTP themselves, so route changes alone do not touch them |

Upgrade the broker and every `swb` client together. A v0.2.x client against
a v0.3.0 broker fails on `inbox` and `end`; the hooks still exit 0, so
a mismatch shows up as silence. `swb doctor` in v0.2.x does not detect it:
it checks `GET /v1/peers`, `/metrics` and optionally `register`, never
`inbox` or `end`. Check `swb version` on every client against the
broker's release instead.
