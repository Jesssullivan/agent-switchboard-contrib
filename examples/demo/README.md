# Scripted demo

`run.sh` drives the whole message flow on `main` (v0.3.0, unreleased) with
scripted fake agents: no model calls, no network beyond `127.0.0.1`, no
secrets. It covers every mode `main` has, in order:

| Step | Mode |
| --- | --- |
| 1 | Broker on loopback, `/healthz`, the Host guard |
| 2 | Hooks: the Claude Code plugin's SessionStart wrapper, saved end token |
| 3–4 | CLI: `whoami --register`, `peers`, `send` |
| 5 | Hooks: the UserPromptSubmit notice |
| 6 | Push channel: `swb channel` as Claude Code sees it, then the `reply` tool |
| 7 | Polling inbox: `swb inbox` and `swb ack` |
| 8 | Authority: an operator-directed claim stays `authority: peer` |
| 9 | MCP over Streamable HTTP: `tools/list`, `register`, `peers`, `send`, `inbox`, `ack` |
| 10 | Cross-host: a second host (a second process with its own `SWB_HOST` and state dir) on the same broker, and a second broker that does not federate |
| 11 | v0.3.0 end tokens: a forged end is refused, SessionEnd spends the token |
| 12 | Claims, release and handoff: **pending P2** stubs only; `swb agentd` exits 3 |

```sh
SWB_BIN=/path/to/swb examples/demo/run.sh            # print the transcript
SWB_BIN=/path/to/swb examples/demo/run.sh --check    # diff against docs/demo/transcript.txt
SWB_BIN=/path/to/swb examples/demo/run.sh --record   # rewrite docs/demo/transcript.txt and demo.cast
```

It needs bash, curl, jq, `mkfifo` and an `swb` 0.3.x binary. Brokers listen
on `127.0.0.1:18181` and `:18182` (metrics `:19191`, `:19192`); move them
with `SWB_DEMO_PORT_BASE`. All run state lives in one `mktemp -d` directory
that the script deletes on exit.

The transcript is reproducible: the script uses fixed message ids and fixed
session identities, and normalizes the few values the broker mints (ULIDs it
generates, timestamps, the run directory, ports). `--check` is the
regression test for the recording.

`--record` writes the asciicast (v2) itself from the normalized transcript,
with fixed synthetic timing, so no recorder is needed and the cast is as
reproducible as the transcript. To record a live terminal session instead:

```sh
asciinema rec --overwrite -c 'examples/demo/run.sh' docs/demo/live.cast
```
