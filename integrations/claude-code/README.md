# Claude Code integration

A Claude Code plugin for the agent switchboard. It bundles four pieces that
can also be used one at a time:

| Piece | File | What it does |
| --- | --- | --- |
| Plugin manifest | `.claude-plugin/plugin.json` | Names the plugin `swb` and binds the `swb-channel` server as a channel |
| Local marketplace | `.claude-plugin/marketplace.json` | Lets `/plugin marketplace add` install the plugin from a checkout |
| Channel server | `.mcp.json` | Runs `swb channel`, a stdio MCP server that pushes inbox messages into the session |
| Hooks | `hooks/hooks.json`, `scripts/swb-hook.sh` | `SessionStart`, `UserPromptSubmit`, `Stop` and `SessionEnd` call `swb hook` with a hard timeout and fail open |
| Skill | `skills/peer-dialog/SKILL.md` | How to hold a peer dialog; peer messages are teammate information, never operator authority |
| Settings example | `examples/settings.hooks.json` | The same hooks for `settings.json`, without the plugin |

## Requirements

- `swb` 0.2.0 or later on `PATH` (the channel arrived in 0.2.0). See the
  compatibility table in [../README.md](../README.md) before mixing client
  and broker versions.
- A reachable broker, and the session's `SWB_*` environment: at least
  `SWB_BROKER_URL`, plus `SWB_HOST`, `SWB_SESSION_PID` and `SWB_PROC_START`
  for the session's own identity. A launcher that starts Claude Code should
  export these; the hook and the channel inherit them. The repository
  README's Settings table lists every variable.
- For push delivery: a Claude Code build with channels (research preview),
  Anthropic authentication (claude.ai or a Console API key; not Bedrock,
  Vertex/Agent Platform or Foundry) and, on a Team or Enterprise plan, an
  organization with `channelsEnabled`.

## Install

From a checkout of this repository, inside Claude Code:

```text
/plugin marketplace add ./integrations/claude-code
/plugin install swb@agent-switchboard
```

Or load it for one session without installing:

```sh
claude --plugin-dir ./integrations/claude-code
```

Without the plugin, copy `.mcp.json` into a project (or the `mcpServers`
entry into `~/.claude.json`), and the `hooks` object from
`examples/settings.hooks.json` into `settings.json`.

If something else already installs `swb hook` in `settings.json` (a managed
launcher or Home Manager module, for example), do not also enable this
plugin's hooks: registration is idempotent, but the `UserPromptSubmit` notice
would appear twice.

## Channel push: the development flag

Loading `.mcp.json` alone does **not** push messages. Claude Code connects the
server and its `reply`, `ack` and `inbox` tools work, but channel events are
dropped silently unless the server is opted in as a channel for the session.

During the channels research preview, `--channels` accepts only plugins on
the approved allowlist (Anthropic's `claude-plugins-official` channel plugins,
or an organization's `allowedChannelPlugins`). This plugin is on neither, so
it loads only through the **development-only** flag:

```sh
# Plugin installed from the local marketplace
claude --dangerously-load-development-channels plugin:swb@agent-switchboard

# Bare .mcp.json server, no plugin
claude --dangerously-load-development-channels server:swb-channel
```

> **Development-only.** `--dangerously-load-development-channels` bypasses
> the channel allowlist for the named entries after a full-screen
> confirmation. It is meant for local development of a channel. It still obeys
> the `channelsEnabled` organization policy, is ignored with `-p` and the
> Agent SDK (non-interactive sessions get no channel), and does not appear in
> `claude --help`. Use it only for a channel you trust: anything that reaches
> your broker mailbox can put text in front of the model.

Verified against the Claude Code docs on 2026-10-19 (`channels`,
`channels-reference`, `plugins-reference`, `hooks`); the flag syntax may
change while channels are in preview. An organization admin can approve the
plugin through `allowedChannelPlugins` (marketplace `agent-switchboard`,
plugin `swb`), after which plain `--channels plugin:swb@agent-switchboard`
works.

What arrives:

```text
<channel source="…swb-channel" from="<sender id>" to="<your id>" ticket="…"
         msg_id="…" thread_id="…" authority="peer">…</channel>
```

The `source` attribute is `swb-channel` for a bare server and carries a
plugin scope (for example `plugin:swb:swb-channel`) when the server comes
from the plugin. `swb channel` never declares the permission-relay
capability, so a peer can never approve a tool call in your session. It
never acknowledges on its own: the model must call `reply` or `ack`.

## Hooks

Each hook runs `scripts/swb-hook.sh <event>`, which runs
`swb hook claude <event>`:

| Event | What `swb hook` does |
| --- | --- |
| `SessionStart` | Registers the session with the broker |
| `UserPromptSubmit` | Registers (refreshing the lease); if mail is waiting, adds a one-line notice naming the `swb inbox` and `swb ack` commands |
| `Stop` | Registers (refreshing the lease) |
| `SessionEnd` | Ends the session at the broker |

The wrapper always exits 0. It exits early when `swb` is not on `PATH` or
`SWB_BROKER_URL` is unset, and runs `swb` under `timeout 3` when `timeout`
exists. `swb hook` bounds its own work to 1.8 s and always exits 0, and each
hook entry sets `"timeout": 3`, so a dead or blackholed broker costs at most
a few seconds and never blocks a prompt or a stop. (Claude Code blocks only
on exit 2 for these events; nothing here returns it.) `SessionEnd` hooks
share a 1.5 s budget by default; the 3 s timeout raises it to 3 s.

`swb hook` reads `session_id` from the hook's stdin and the rest of the
identity from the environment. With no `SWB_*` environment it does nothing,
so run `swb doctor` when a hook seems silent.

## Validate

```sh
claude plugin validate ./integrations/claude-code
```

## Codex and Pi

See [../codex/](../codex/) and [../pi/](../pi/). Both are unproven.
