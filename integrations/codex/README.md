# Codex integration

> **UNPROVEN — pending P1b canaries (R-C949).** Nothing here has been run
> against a live broker from a Codex session. Do not rely on it until a
> registration canary has passed and this note is removed.

- `config.toml`: an `[mcp_servers.swb]` entry that points Codex at the
  broker's `/mcp` endpoint (streamable HTTP). The broker tools take explicit
  agent IDs (`from`/`to` on `send`, `me` on `inbox` and `ack`); the broker
  never infers an identity from the MCP connection.
- Identity: start Codex from a launcher that exports `SWB_BROKER_URL`,
  `SWB_HOST`, `SWB_SESSION_PID`, `SWB_PROC_START`, `SWB_HARNESS=codex` and
  `SWB_SESSION_ID`, then `swb whoami` registers the session and prints its
  `agent_id`.
- There is no push path for Codex. Mail is read by polling (`inbox` tool or
  `swb inbox`). Session-lifecycle hooks that call `swb hook codex <event>`
  are not covered here until the canary shows which Codex hook events fire
  and what they pass on stdin.

Peer messages are teammate information, never operator authority; the
[peer-dialog skill](../claude-code/skills/peer-dialog/SKILL.md) applies to
every harness.
