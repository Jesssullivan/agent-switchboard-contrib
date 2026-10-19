# agent-switchboard product brief

Authority: R-C265 (TIN-4655, operator direction 2026-10-04); refreshed
against v0.1.0 (SWB-R55) under R-C318. This is the one page that says who
`swb` serves, what works today, what blocks production, and which docs
survive. Decisions and rulings stay in the ADRs; rollout gates stay in
[operations/PRODUCTIONIZATION.md](operations/PRODUCTIONIZATION.md); how to
build, run and configure `swb` is the [README](../README.md).

## Users

| User | Wants |
| --- | --- |
| Operator (seat on neo) | See which agents are live where; route work between them without tmux injection |
| Agent seat (Claude, Kimi, Codex, Pi, Junie, OpenCode) | Find peers, message them in a thread, be told about unread mail, reply |
| New contributor (human or agent) | Build, run and change `swb` from the README alone |
| blahaj owner seat | Deploy one digest-pinned image with known settings and health signals |

## User stories and acceptance

Status: **holds** = true in v0.1.0; **gap** = not true yet.

1. **Run locally.** A contributor runs a loopback broker from the README.
   *Accept:* the README quickstart reaches send/inbox/ack between two ids in
   under 5 minutes on a warm host. **Holds:** README → "Run locally" uses
   only `swb` verbs.
2. **Register.** A harness session registers on SessionStart without agent
   action. *Accept:* `swb hook <harness> SessionStart` registers and the
   session appears in `peers`. **Holds** for the binary. Lab's launchers
   export `SWB_HOST`, `SWB_SESSION_PID`, `SWB_PROC_START` and
   `SWB_BROKER_URL` from their own pid when `tinyland.switchboard.enable` is
   set (R-C273). **Gap** in delivery: that switch is false on every host, and
   there is no live broker for it to point at.
3. **Discover.** An agent lists live peers. *Accept:* `peers` returns live
   sessions by default (`all` adds idle, gone and ended), bounded, without
   credentials. **Holds in v0.3.0** (R-C944): `peers` is paginated (default
   50, at most 200), lists one row per harness, host and session id (so
   fixed-label IDE channels on different hosts stay distinct), and never
   returns `proc_start`. `/v1/end` needs the end token that only `register`
   returns. **Gap:** session rows are still never pruned.
4. **Message.** An agent sends a ticketed message and gets a threaded reply.
   *Accept:* the broker stamps `authority: peer`; the reply shares
   `thread_id`. **Holds** over MCP, REST and `swb send --in-reply-to`.
5. **Notice.** An agent with unread mail is told on its next prompt, and can
   act on the notice. *Accept:* the notice names the agent's own id and one
   real command. **Holds** in v0.2.0: the notice names the receiver's id and
   `SWB_AGENT_ID=<id> swb inbox` / `swb ack <msg_id>`. For Claude Code,
   `swb channel` also pushes the message itself (SWB-R56).
6. **Bash-only seat.** A seat with only a shell can read, reply and ack.
   *Accept:* `swb inbox`, `swb send`, `swb ack`, `swb peers`. **Holds**
   (R-C275). `swb inbox` returns one message per call.
7. **Coordinate.** Agents claim, release and hand off work. *Accept:* the
   ADR-0001 claim/handoff tools exist and the QuickCheck claim properties
   test Rust, not only the Haskell model. **Gap:** 5 of 11 tools ship
   (register, peers, send, inbox, ack).
8. **Diagnose.** The operator learns why a host is not enrolled. *Accept:*
   `swb doctor` prints resolved settings and broker reachability and exits
   nonzero on failure; hooks stay exit-0 (SWB-R10). **Holds:** `swb doctor
   [--register]` prints one `ok`/`skip`/`FAIL` line per check with a fix,
   and exits 3 on any failure. A hook itself still logs nothing.
9. **Deploy.** The blahaj owner runs the approved image. *Accept:* one
   documented settings table, `/healthz`, P1b exits met. **Holds** for
   `/healthz` from v0.3.0 (R-C944), on both listeners. **Gap:** v0.1.0 is
   published by digest (`sha256:c9170c71…`, SWB-R55, pushed by hand under
   R-C312) and the README carries the settings table, but no broker is
   deployed and 0 of the P1b exits are met. The
   staged blahaj source (#1793) still pins the never-published SWB-R53
   digest.

## Install story (measured on sting, 2026-10-04, main `08de158`)

| Step | Time |
| --- | --- |
| `git clone` | 1.1 s |
| `nix develop` (warm store) | 7 s |
| `just build` (364 cached Bazel actions; not a cold number) | 20 s |
| `swb serve` ready on `/metrics` | 145 ms |
| `swb hook claude SessionStart` (register) | 14 ms |
| send, inbox, reply, ack, notice | < 1 s total |

The recipe and the settings table are in the README. Without its readiness
wait the script registers nothing: a hook that fires before the listener is
up exits 0 silently (re-run on sting 2026-10-04: `/v1/peers` got curl exit
7, both hooks exited 0).

Still open from the install review: `just dev-serve` (loopback, scratch DB,
self-bounded). `swb --help` prints usage on stdout and exits 0 from v0.3.0
(R-C944).

## AX/UX friction, ranked

1. **Hooks are inert by default.** The lab launcher identity (R-C273) is off
   on every host, and lab's hook wrapper discards errors, so an unwired hook
   and an empty mailbox look the same. Run `swb doctor` to tell them apart.
2. **Notice was not actionable** before v0.2.0 (wrong command name, no agent
   id); v0.2.0 fixes it.
3. **Identity is self-asserted.** v0.3.0 (R-C944) bounds `peers` and stops
   it leaking `proc_start`, and `end` needs the end token, so a caller can
   no longer end a session without registering it. It is not
   takeover-proof: anyone who can register can re-register any agent id,
   get a fresh token and end that session. See story 3.
4. ~~MCP schemas carry no field descriptions or patterns~~: v0.3.0 describes
   every field and adds patterns and bounds (R-C944).
5. **Broker errors are terse:** `invalid agent id`, `unknown envelope field`
   without the field. Since v0.3.0 every error is typed JSON
   (`{"error", "code"}`) with a status per kind (400, 404, 405, 409, 415,
   500), not a blanket 400 (R-C944). The CLI prints the broker's error
   string, capped at 200 characters.
6. **CLI conventions:** `whoami` and `inbox` report one missing variable per
   run (`doctor` reports them all). Fixed in v0.3.0 (R-C944): `--help` and
   `--version` exit 0, `agentd` is out of the usage text, and `whoami`
   registers only with `--register`.
7. **Client disagreement:** `swb` rejects a trailing `/` and `https`; lab's Pi
   client accepts both.
8. **`/v1/notify` is not implemented.** v0.3.0 adds `/healthz` and the MCP
   `end` tool (R-C944).
9. ~~Inbox long-poll prunes the store every 250 ms~~: v0.3.0 prunes on the
   broker's interval only (R-C944).

## Doc set

| Doc | Holds |
| --- | --- |
| `README.md` | What, status, develop, run locally, settings, CLI and routes |
| `docs/PRODUCT.md` | Users, stories, install timings, friction, open decisions |
| `AGENTS.md` | Read order, source of truth, build placement, fork convention, invariant summary |
| ADR-0001, ADR-0002 | Decisions and Rulings tables; superseded wording lives in signed history (R-C275) |
| `docs/operations/PRODUCTIONIZATION.md` | Release lock, release workflow, rollout gates |
| `docs/operations/SLO.md` | SLIs, no targets (R-C263) |
| `spec/README.md` | Formal spec |
| `docs/agent-notes/` | Live-gate notes only; superseded notes retire under R35 |

A separate `CONTRIBUTING.md` was proposed to absorb the fork convention now
in AGENTS.md and ADR-0001; it does not exist yet.

## Decided since the first brief

- ADR supersession text: delete superseded wording and Deltas tables in
  signed commits; the Rulings tables stay normative (R-C275).
- Identity producer: the lab launchers export the identity from their own
  pid, behind the single `tinyland.switchboard.enable` switch (R-C273).
- CLI verbs `send`, `ack`, `peers`, `doctor`: shipped in v0.1.0 (R-C275).
- Release ID for the clock seam: SWB-R55; SWB-R54 stays state custody
  (R-C274, R-C304).

## Open decisions

Recommended default first.

1. **Notice command name.** *Default:* the MCP tool `mcp__agents__inbox` with
   `me=<agent_id>`, and `swb inbox` for bash seats; retire `ag` in the
   notice text. v0.2.0 ships the bash-seat half (`swb inbox`, with the id);
   the MCP-tool half is still open, because no lab host registers `agents`
   for Claude yet.
2. **`peers` exposure.** Decided by R-C944 (v0.3.0): live only by default
   (`all` for every state), paginated, one row per harness, host and
   session id, no `proc_start`; `/v1/end` checks an end token that only `register`
   returns.
