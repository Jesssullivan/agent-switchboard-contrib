# Scripted demo

`run.sh` drives the whole message flow on `main` (v0.3.0, unreleased) with
scripted fake agents: no model calls, no network beyond `127.0.0.1`, no
secrets. It covers every mode `main` has, in order:

| Step | Mode |
| --- | --- |
| 1 | Broker on loopback, `/healthz`, the Host guard |
| 2 | Hooks: the Claude Code plugin's SessionStart wrapper, saved end token |
| 3–4 | CLI: `whoami --register`, `peers`, `send` |
| 5 | Hooks: the Claude Code UserPromptSubmit notice (Codex hooks are unproven; Pi has no hook surface) |
| 6 | Push channel: `swb channel` as Claude Code sees it, then the `reply` tool |
| 7 | Polling inbox: `swb inbox` and `swb ack` |
| 8 | Authority: an operator-directed claim stays `authority: peer` |
| 9 | MCP over Streamable HTTP, with curl as the client: `tools/list`, `register`, `peers`, `send`, `inbox`, `ack`. The agent registers with harness id `pi`, but Pi has no built-in MCP client and its integration is unproven (R-C949) |
| 10 | Cross-host: a second host (a second process with its own `SWB_HOST` and state dir) on the same broker, and a second broker that does not federate |
| 11 | v0.3.0 end tokens: a forged end is refused, SessionEnd spends the token |
| 12 | Claims and handoff as ADR-0001 SWB-R16 and SWB-R60 (R-C950) define them (advisory claims with `overlaps[]`, an exclusive request refused while any other session holds an active claim on the subject, `offered` handoff receipt accepted by the receiver's claim): **pending P2** stubs only; `swb agentd` exits 3 |

```sh
SWB_BIN=/path/to/swb examples/demo/run.sh            # print the transcript
SWB_BIN=/path/to/swb examples/demo/run.sh --check    # diff against docs/demo/transcript.txt
SWB_BIN=/path/to/swb examples/demo/run.sh --record   # rewrite docs/demo/transcript.txt and demo.cast
```

It needs bash, curl, jq, `mkfifo` and an `swb` 0.3.x binary. Brokers listen
on `127.0.0.1:18181` and `:18182` (metrics `:19191`, `:19192`); move them
with `SWB_DEMO_PORT_BASE`. The script refuses to start if any of those ports
already answers, and treats a broker as up only while its own process is
alive. That is a best-effort guard, not a guarantee: a listener that accepts
but never answers HTTP still reads as a free port. All run state lives in one `mktemp -d` directory
that the script deletes on exit.

The transcript is reproducible: the script uses fixed message ids and fixed
session identities, and normalizes the few values the broker mints (ULIDs it
generates, timestamps, the run directory, ports). `--check` diffs a fresh run
against the recording. `just demo-check` builds `swb` with Bazel and runs it
together with the one-host selftest, on a build host. CI does not run either
yet, so rerun `just demo-check` (and `--record` when output changes on
purpose) in any PR that changes tool or CLI output.

`--record` writes the asciicast (v2) itself from the normalized transcript,
with fixed synthetic timing, so no recorder is needed and the cast is as
reproducible as the transcript. To record a live terminal session instead:

```sh
asciinema rec --overwrite -c 'examples/demo/run.sh' docs/demo/live.cast
```
