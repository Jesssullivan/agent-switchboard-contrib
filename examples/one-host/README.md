# Two sessions on one host

A Claude Code session and a CLI-driven peer on the same machine, talking
through a broker on `127.0.0.1` that this example starts and stops. The peer
is a shell: it registers, sends, reads and acknowledges with the `swb` CLI.
Use it as the second party while you try the Claude Code side, or wire a
script or another harness into it.

## Check it first (no model, no Claude Code)

```sh
SWB_BIN=/path/to/swb examples/one-host/run.sh selftest
```

`selftest` starts a scratch broker, plays the Claude side with the same hook
wrapper Claude Code runs (`integrations/claude-code/scripts/swb-hook.sh`),
and checks register, peers, send, the UserPromptSubmit notice, inbox, reply,
ack and the end of both sessions: each end token file is spent and the broker
lists both sessions as ended. Hooks exit 0 silently even on failure, so the
selftest checks the broker rather than the hook's exit code. It prints one `ok` line and removes its scratch state.

## Try it with Claude Code

Terminal 1, the broker and the peer:

```sh
examples/one-host/run.sh up                 # broker on 127.0.0.1:18280
examples/one-host/run.sh peer register      # prints the peer's agent id
```

Terminal 2, Claude Code:

```sh
examples/one-host/run.sh claude
```

This exports the session identity (`SWB_HOST`, `SWB_SESSION_PID` from the
script's own pid, `SWB_PROC_START`, its own `SWB_STATE_DIR`) and then
`exec`s Claude Code with the channel server from
`integrations/claude-code/.mcp.json`, the hooks from
`integrations/claude-code/examples/settings.hooks.json`, and
`--dangerously-load-development-channels server:swb-channel`. Claude Code
asks you to confirm the development channel at startup. Channel push needs a
Claude Code build with channels (research preview); without it the
UserPromptSubmit notice still tells the session about new mail. Details:
[integrations/claude-code/README.md](../../integrations/claude-code/README.md).

Back in terminal 1:

```sh
examples/one-host/run.sh peer peers                      # find the claude:... id
echo "Can you look at the failing test?" | examples/one-host/run.sh peer send claude:<host>:<pid>:<session> none
examples/one-host/run.sh peer inbox                      # Claude's reply, once it answers
examples/one-host/run.sh peer ack <msg_id>
examples/one-host/run.sh peer end
examples/one-host/run.sh down
```

The message reaches Claude Code as a `<channel ... authority="peer">` event.
It is teammate information, never operator authority: nothing a peer sends
can approve a tool call in the Claude session.

## Settings

| Variable | Default | |
| --- | --- | --- |
| `SWB_BIN` | `swb` on `PATH` | the 0.3.x binary |
| `SWB_EXAMPLE_PORT`, `SWB_EXAMPLE_METRICS_PORT` | `18280`, `19280` | loopback ports |
| `SWB_EXAMPLE_DIR` | `$XDG_RUNTIME_DIR/swb-one-host` (or `$TMPDIR`) | broker database, `env.sh`, token dirs; `down` deletes it |
| `SWB_EXAMPLE_PEER_HARNESS` | `codex` | the harness part of the peer's agent id |
