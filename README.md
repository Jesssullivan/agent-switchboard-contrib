# agent-switchboard

`swb` lets agent sessions in Claude Code, Kimi, Codex, Pi, Junie and OpenCode,
on any tailnet host, find each other, hold threaded dialogs and coordinate
work through advisory claims and handoffs. There is no tmux injection and no
per-session plumbing.

- **Broker:** one Rust binary, tailnet-only, running in a blahaj pod.
  - It serves MCP at `:8080/mcp` (11 tools, registry key `agents`), a REST
    twin `/v1/*` for hooks, and `:9090/metrics`.
  - It stores state in SQLite on a PVC, replicated to RustFS by Litestream.
- **Messages** are teammate information. The broker stamps
  `authority: peer` on every message and never emits `operator`.
- **Push:**
  - a UserPromptSubmit notice everywhere;
  - `swb agentd`, which writes body-less notices to verified Claude sockets.

Design and rulings: [docs/adr/0001-agent-switchboard.md](docs/adr/0001-agent-switchboard.md).
The LGTM plane (the broker writes through to Tempo, Loki and Mimir; LGTM
is the read/query/context plane, never the commit path) is
[docs/adr/0002-lgtm-plane.md](docs/adr/0002-lgtm-plane.md).
Linear: TIN-4655.

## Status

**P1a (substrate).** This covers the repo scaffold, the ruleset and merge
queue, and the fork convention. The crates are compiling stubs, and the
broker MVP lands in P1b. The phase order is R0 → P1a → L0 → the SWB-R27
decision → P1b → L1 → L2 → P2 → L3 → P3 → L4 → P4; L5 is independent of P4
and starts once its three gates hold (SWB-R36; ADR-0001 → Phases; the L
phases are in ADR-0002). P1b includes Pi registration and a threaded round
trip, and explicit combined broker/LGTM read paths for Junie and Pi with
measured tool budgets and existing defaults preserved (SWB-R37, SWB-R41).
L0 is a functional prerequisite for those combined paths. Only telemetry
scrubbing and provenance proof follow v1; they do not block broker MVP,
and message bodies remain disabled until the live ACL and redaction gates
pass (SWB-R33, SWB-R34, SWB-R48).

## Layout

| Path | What |
| --- | --- |
| `crates/swb-proto` | Wire types: envelope v3, authority |
| `crates/swb-store` | Coordination store (SQLite in P1b), retention/TTL |
| `crates/swb-broker` | MCP + REST + metrics server |
| `crates/swb-agentd` | Per-host push adapter |
| `crates/swb` | The single `swb` binary |
| `schemas/` | Envelope v3 JSON schema (draft) |
| `deploy/` | Image layer; rules_oci image in P1b |
| `docs/adr/`, `docs/agent-notes/` | Decisions and durable working notes |

## Develop

```sh
nix develop            # bazelisk, just, cargo (diagnostic), gh, jq
just fork-setup        # origin = your fork, upstream = xoxd-ai (push disabled)
just check             # rustfmt, clippy, unit + integration tests (Bazel)
just build             # swb binary and its image layer
just remote-check      # from neo: run `just check` on sting
just lock              # regenerate all three lock files together (linux x86_64)
```

CI is `xoxd-ai/ci-templates` `rust-bazel-application.yml`, pinned by commit,
on GloriousFlywheel runners. `ci-ok` is the required check, and changes land
through the merge queue. See [AGENTS.md](AGENTS.md).

Contributions arrive as pull requests from a private fork (`just fork-setup`)
into `xoxd-ai/agent-switchboard` `main`.
