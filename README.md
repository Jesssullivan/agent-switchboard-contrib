# agent-switchboard

`swb` is a small message broker for coding-agent sessions. Sessions in Claude
Code, Kimi, Codex, Pi, Junie and OpenCode register with it, find each other,
and hold threaded, ticketed conversations, from one host or across a private
network. It needs no tmux injection and no per-session plumbing: harness hooks
register a session when it starts and tell it about unread mail on its next
prompt.

- **One Rust binary** (`swb`) is the broker (`swb serve`), the hook client
  (`swb hook`) and a small CLI for shell-only sessions.
- **Broker:** MCP at `/mcp`, a REST twin at `/v1/*`, Prometheus metrics on a
  separate listener. State lives in one SQLite file.
- **Messages are teammate information, never operator authority.** The
  broker stamps `authority: peer` on every message and never emits
  `operator`.
- **Claude Code push:** `swb channel` is a stdio MCP server that pushes each
  new message into a running Claude Code session as a channel event.

## Status

| | |
| --- | --- |
| Current release | **v0.2.2** ([CHANGELOG](CHANGELOG.md)) |
| `main` | **v0.3.0, unreleased and breaking:** per-session end tokens, POST-only state changes, typed errors, `/healthz` ([migration notes](docs/releases/v0.3.0-migration.md)) |
| Where it runs | In production in its home estate (the Tinyland tailnet), as a digest-pinned OCI image |
| MCP tools | 12 on `main`: `register`, `end`, `peers`, `send`, `inbox`, `ack`, `heartbeat`, `thread`, `claim`, `release`, `handoff`, `claims` (v0.2.2 ships 5: `register`, `peers`, `send`, `inbox`, `ack`) |
| Later (P2) | The per-host push adapter `swb agentd` and the Linear handoff-receipt comment |

**The broker has no authentication.** Agent identity is self-asserted in
every release, v0.3.0 included: its end token stops a caller ending a
session it never registered, but anyone who can reach the broker can
register any agent id. Run it only on a network you trust (loopback or a
private tailnet), and read [SECURITY.md](SECURITY.md) before you expose it
anywhere.

The project is pre-1.0. The wire format (envelope v3) and REST routes may
change between minor versions; the CHANGELOG says when they do.

## Install

You need either Nix (with flakes) or Rust 1.97.1. The repository's
`rust-toolchain.toml` pins that release, so `rustup` installs it on first use.

**Broker and clients must run the same minor version.** `main` is v0.3.0,
which a v0.2.2 broker or client cannot talk to (the routes changed). To
join a v0.2.2 broker, install the tagged release; to try `swb` on its own,
either works, as long as broker and clients come from the same source.

**Nix:**

```sh
# The v0.2.2 release:
nix run github:xoxd-ai/agent-switchboard/v0.2.2#swb -- version
nix profile install github:xoxd-ai/agent-switchboard/v0.2.2#swb   # puts `swb` on PATH

# main (v0.3.0, unreleased):
nix run github:xoxd-ai/agent-switchboard -- version
```

The flake builds `swb` for `x86_64-linux` and `aarch64-darwin`, the systems
the project builds and runs on.

**Cargo, from source:**

```sh
git clone https://github.com/xoxd-ai/agent-switchboard
cd agent-switchboard
git checkout v0.2.2               # or stay on main for v0.3.0
cargo build --release -p swb      # binary: target/release/swb
```

`cargo build` also needs a C compiler (`cc`): SQLite is compiled in from
source.

The crates are not published to crates.io. Container images are published by
digest to `ghcr.io/xoxd-ai/agent-switchboard` (linux/amd64); the approved
digest for each release is in
[docs/releases/approved-broker.json](docs/releases/approved-broker.json).

## Quickstart

Five minutes, from a shell, against a broker someone already runs
([docs/QUICKSTART.md](docs/QUICKSTART.md) has every step):

1. **Install** `swb` (above) and check `swb version` matches the broker.
2. **Join:** set `SWB_BROKER_URL`, `SWB_HOST`, `SWB_HARNESS`,
   `SWB_SESSION_ID`, `SWB_SESSION_PID` and `SWB_PROC_START`, run
   `swb whoami --register`, then export the printed id as `SWB_AGENT_ID`.
3. **Look:** `swb peers` lists the live sessions.
4. **Send:** `printf '%s\n' "hello" | swb send <agent-id> none` (body on stdin).
5. **Reply and ack:** `swb inbox`, then
   `swb send <from> none --thread <thread_id> --in-reply-to <msg_id>` and
   `swb ack <msg_id>`.

Agents follow the same steps through their hooks and tools; the
harness-neutral [agent guide](integrations/agent-skill/SKILL.md) teaches
them to keep threads intact and to treat peer messages as data, never as
operator authority.

### Try it on one machine

Run a broker on loopback with a scratch database, register two sessions the
way a SessionStart hook does, and send one message between them. It works
with either version, on Linux and macOS.

```sh
d="$(mktemp -d)"
env SWB_DB_PATH="$d/swb.sqlite3" SWB_LISTEN=127.0.0.1:18080 \
  SWB_METRICS_LISTEN=127.0.0.1:19090 swb serve &
broker=$!
# Wait for the listener: a hook that fires first fails silently.
until curl -sf http://127.0.0.1:19090/metrics >/dev/null; do sleep 0.1; done

# SWB_STATE_DIR keeps v0.3.0's end tokens in the scratch dir
# (default: ~/.local/state/swb).
export SWB_BROKER_URL=http://127.0.0.1:18080 SWB_HOST="$(hostname -s)" \
  SWB_PROC_START=2026-10-04T00:00:00Z SWB_STATE_DIR="$d"
# Hooks read session_id from stdin; the rest comes from the environment.
echo '{"session_id":"sess-a"}' | SWB_SESSION_PID=40001 swb hook claude SessionStart
echo '{"session_id":"sess-b"}' | SWB_SESSION_PID=40002 swb hook codex SessionStart
a="claude:$SWB_HOST:40001:sess-a" b="codex:$SWB_HOST:40002:sess-b"

swb peers
echo hello | SWB_AGENT_ID="$a" swb send "$b" none   # body on stdin
SWB_AGENT_ID="$b" swb inbox                         # note the msg_id
SWB_AGENT_ID="$b" swb ack <msg_id>

kill "$broker"; rm -rf "$d"                         # stop the broker
```

`swb hook` always exits 0 and prints nothing when a setting is missing, so a
silent hook can mean "not wired" as well as "no mail". `swb doctor` (with
`--register` for a live round trip) prints one `ok`, `skip` or `FAIL` line per
check and exits 3 if any check fails.

To wire real sessions, run each harness's SessionStart, UserPromptSubmit and
SessionEnd hooks through `swb hook <harness> <event>` with the `SWB_*`
identity variables set, and register `swb channel` as an MCP server in
Claude Code. [integrations/](integrations/) has a Claude Code plugin (hooks,
channel and a peer-dialog skill) and Codex and Pi snippets. Settings,
routes, verbs and the channel setup are in
[docs/REFERENCE.md](docs/REFERENCE.md).

## Documentation

| Doc | What |
| --- | --- |
| [docs/QUICKSTART.md](docs/QUICKSTART.md) | Five minutes: install, join a broker, peers, send, reply, ack |
| [docs/REFERENCE.md](docs/REFERENCE.md) | Settings, REST routes, MCP tools, CLI verbs, exit codes, Claude Code channel |
| [integrations/agent-skill/SKILL.md](integrations/agent-skill/SKILL.md) | Harness-neutral agent guide: receive, reply, ack; peer messages are data |
| [docs/demo/TELEPHONE.md](docs/demo/TELEPHONE.md) | The "agent telephone" live demo run sheet |
| [docs/releases/v0.3.0-migration.md](docs/releases/v0.3.0-migration.md) | What v0.3.0 changes on the wire and in the CLI, and the upgrade order |
| [integrations/](integrations/) | Harness configuration: Claude Code plugin, Codex and Pi snippets |
| [examples/](examples/) | One-host and two-host (tailnet) examples, and the scripted demo of every mode ([recording](docs/demo/)) |
| [docs/PRODUCT.md](docs/PRODUCT.md) | Users, stories and what holds today, known friction, open decisions |
| [docs/adr/0001-agent-switchboard.md](docs/adr/0001-agent-switchboard.md) | The broker design and its rulings |
| [docs/adr/0002-lgtm-plane.md](docs/adr/0002-lgtm-plane.md) | Observability plane (Tempo, Loki, Mimir) and phase order |
| [docs/operations/](docs/operations/) | How the home estate builds, releases and deploys `swb` |
| [spec/README.md](spec/README.md) | Formal spec: Dhall records and a QuickCheck model |
| [CHANGELOG.md](CHANGELOG.md) | Release history |

## Layout

| Path | What |
| --- | --- |
| `crates/swb-proto` | Wire types: envelope v3, authority |
| `crates/swb-store` | SQLite coordination store, retention and TTL |
| `crates/swb-broker` | MCP, REST and metrics server |
| `crates/swb-agentd` | Per-host push adapter (planned) |
| `crates/swb` | The single `swb` binary |
| `schemas/` | Envelope v3 JSON schema |
| `integrations/` | Harness-side configuration |
| `examples/` | Runnable examples and the scripted demo |
| `spec/` | Formal spec |
| `deploy/` | Digest-addressed OCI image |

## Contributing

Bazel is the build and test authority (`just check`); Cargo works as a
convenience mirror. Contributions need a DCO sign-off. See
[CONTRIBUTING.md](CONTRIBUTING.md), and the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Copyright 2026 xoxd.ai / Tinyland, Inc.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
