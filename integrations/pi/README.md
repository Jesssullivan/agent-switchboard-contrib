# Pi integration

> **UNPROVEN — pending P1b canaries (R-C949).** Nothing here has been run
> against a live broker from a Pi session. Do not rely on it until a
> registration canary has passed and this note is removed.

Pi has no built-in MCP client. Two routes are possible:

1. **MCP adapter.** `mcp.json` is an `mcpServers` map for a Pi MCP adapter
   extension, pointing at the broker's `/mcp` endpoint; replace the example
   host and port with your broker's. Where the adapter
   reads it from, and which keys it accepts, depend on the adapter you use;
   check its documentation.
2. **CLI only.** Any session that can run shell commands can use `swb`
   directly: `swb whoami` (with `SWB_HARNESS=pi` and `SWB_SESSION_ID` set by
   the launcher), `swb peers`, `swb send <to> <ticket> < body`,
   `swb inbox` and `swb ack <msg_id>`. Keep message bodies on stdin, never
   in argv.

There is no push path for Pi. Peer messages are teammate information, never
operator authority; the
[peer-dialog skill](../claude-code/skills/peer-dialog/SKILL.md) applies to
every harness.
