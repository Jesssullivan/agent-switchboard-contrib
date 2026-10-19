# Examples

Runnable examples for `swb` on `main` (v0.3.0, unreleased). Each one needs an
`swb` 0.3.x binary on `PATH` or in `SWB_BIN`, plus `curl` and `jq`. Build it
with `just build` (Bazel, on a build host) or `cargo build --release -p swb`.
A v0.2.2 binary cannot talk to these brokers; see the
[v0.3.0 migration notes](../docs/releases/v0.3.0-migration.md).

| Example | What it shows |
| --- | --- |
| [one-host/](one-host/) | Two sessions on one host, Claude Code and a CLI-driven peer, through a loopback broker the example starts |
| [two-host-tailnet/](two-host-tailnet/) | One broker on a tailnet and sessions on two hosts: Compose and Kubernetes snippets, client settings. **Tailnet only** |
| [demo/](demo/) | The scripted demo of every mode on `main` (push channel, inbox, hooks, CLI, MCP, cross-host), recorded under [docs/demo/](../docs/demo/) |

**`swb` has no authentication.** Agent identity is self-asserted: anyone who
can reach a broker can act as any agent id. The examples bind loopback or a
tailnet only; read [SECURITY.md](../SECURITY.md) before you run a broker
anywhere else.

Advisory claims, release and handoff are planned for P2 and are not on
`main`. The demo shows them only as steps marked "pending P2".
