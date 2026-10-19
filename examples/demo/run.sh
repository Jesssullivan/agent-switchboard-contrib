#!/usr/bin/env bash
# Scripted, reproducible demo of every swb mode on main (v0.3.0, unreleased).
#
# R-C953 (TIN-5770 comment fff218eb): recorded demos now, a sandbox later.
# Everything here is local and deterministic: two loopback brokers, scripted
# fake agents, fixed message ids, no model calls, no network beyond
# 127.0.0.1 and no secrets. Advisory claims and handoff are P2 and appear only
# as clearly marked "pending P2" stubs. One machine plays both hosts.
#
# Usage:
#   examples/demo/run.sh            run the demo, print the transcript
#   examples/demo/run.sh --check    run it and diff against docs/demo/transcript.txt
#   examples/demo/run.sh --record   run it and rewrite docs/demo/transcript.txt
#                                   and docs/demo/demo.cast (asciicast v2)
#
# Needs: bash, curl, jq, mkfifo and an `swb` 0.3.x binary, from SWB_BIN or PATH.
# Ports: 127.0.0.1:18181/19191 (broker A) and 18182/19192 (broker B); set
# SWB_DEMO_PORT_BASE to move them (A = base+1, B = base+2, metrics +1010).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
mode="${1:-run}"
case "$mode" in
  run | --check | --record) ;;
  -h | --help) sed -n '2,19p' "$0"; exit 0 ;;
  *) echo "usage: $0 [--check|--record]" >&2; exit 2 ;;
esac

swb_bin="${SWB_BIN:-$(command -v swb || true)}"
[ -x "$swb_bin" ] || { echo "demo: no swb binary (set SWB_BIN)" >&2; exit 3; }
for tool in curl jq mkfifo; do
  command -v "$tool" >/dev/null || { echo "demo: $tool is required" >&2; exit 3; }
done
case "$("$swb_bin" version)" in
  "swb 0.3."*) ;;
  *) echo "demo: needs swb 0.3.x (main); got: $("$swb_bin" version)" >&2; exit 3 ;;
esac

base="${SWB_DEMO_PORT_BASE:-18180}"
port_a=$((base + 1)) port_b=$((base + 2))
metrics_a=$((port_a + 1010)) metrics_b=$((port_b + 1010))
broker_a="http://127.0.0.1:$port_a" broker_b="http://127.0.0.1:$port_b"

# Transient working tree for this run only (databases, fifos, token dirs).
# The durable outputs are the committed transcript and cast under docs/demo/.
demo="$(mktemp -d "${TMPDIR:-/tmp}/swb-demo.XXXXXX")"
pids=()
cleanup() {
  { exec 7>&-; } 2>/dev/null || true
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || true; done
  rm -rf "$demo"
}
trap cleanup EXIT

# `swb` on PATH under its own name: the Claude Code plugin's hook wrapper
# looks it up that way.
mkdir -p "$demo/bin"
ln -s "$swb_bin" "$demo/bin/swb"
export PATH="$demo/bin:$PATH"
unset SWB_AGENT_ID SWB_HARNESS SWB_SESSION_ID SWB_METRICS_URL || true

# ---------------------------------------------------------------- helpers
raw="$demo/raw.txt"
: >"$raw"
out() { printf '%s\n' "$*" >>"$raw"; }
step() { out ""; out "## $*"; out ""; }
say() { out "# $*"; }
# show "<display>" cmd...: print the display line, run cmd, print its output.
show() {
  local display="$1"
  shift
  out "\$ $display"
  local rc=0
  "$@" >>"$raw" 2>&1 || rc=$?
  [ "$rc" -eq 0 ] || out "[exit $rc]"
}
# Fixed ids keep the transcript reproducible (ULID alphabet, first char <= 7).
M1=01JDEM0000000000000000000A
M2=01JDEM0000000000000000000B
M3=01JDEM0000000000000000000C
M4=01JDEM0000000000000000000D

port_free() { # port: nothing may answer there before our broker starts
  ! curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$1/" 2>/dev/null
}
start_broker() { # name port metrics
  local name="$1" port="$2" metrics="$3" pid
  for p in "$port" "$metrics"; do
    port_free "$p" || {
      echo "demo: 127.0.0.1:$p is already in use; set SWB_DEMO_PORT_BASE" >&2
      exit 3
    }
  done
  env SWB_DB_PATH="$demo/$name.sqlite3" SWB_LISTEN="127.0.0.1:$port" \
    SWB_METRICS_LISTEN="127.0.0.1:$metrics" "$swb_bin" serve \
    >"$demo/$name.log" 2>&1 7>&- &
  # 7>&-: a broker started after the channel's stdin (fd 7) is open must not
  # hold its write end, or closing fd 7 never reaches the channel as EOF.
  pid="$!"
  pids+=("$pid")
  # Healthy only while our own process is alive: a broker that failed to
  # bind exits, and a foreign listener must never pass for it.
  for _ in $(seq 1 100); do
    kill -0 "$pid" 2>/dev/null || break
    if curl -sf "http://127.0.0.1:$metrics/healthz" >/dev/null 2>&1 &&
      curl -sf "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then
      kill -0 "$pid" 2>/dev/null && return 0
    fi
    sleep 0.1
  done
  echo "demo: broker $name did not come up; its log:" >&2
  cat "$demo/$name.log" >&2
  exit 1
}

# Session environments. Each fake agent is a process with its own SWB_*
# identity and its own token directory, as a real launcher would give it.
as_alice() { # Claude Code on host-a
  env SWB_BROKER_URL="$broker_a" SWB_HOST=host-a SWB_SESSION_PID=40001 \
    SWB_PROC_START=2026-10-19T09:00:00Z SWB_STATE_DIR="$demo/state-alice" "$@"
}
as_bob() { # a shell-driven peer (CLI only) on host-a
  env SWB_BROKER_URL="$broker_a" SWB_HOST=host-a SWB_SESSION_PID=40002 \
    SWB_PROC_START=2026-10-19T09:00:01Z SWB_STATE_DIR="$demo/state-bob" \
    SWB_HARNESS=codex SWB_SESSION_ID=sess-bob SWB_AGENT_ID="$BOB" "$@"
}
as_carol() { # harness id "pi" on host-b, same broker over the (simulated) tailnet
  env SWB_BROKER_URL="$broker_a" SWB_HOST=host-b SWB_SESSION_PID=50001 \
    SWB_PROC_START=2026-10-19T09:00:02Z SWB_STATE_DIR="$demo/state-carol" "$@"
}
ALICE=claude:host-a:40001:sess-alice
BOB=codex:host-a:40002:sess-bob
CAROL=pi:host-b:50001:sess-carol

hook_json() { printf '{"session_id":"%s"}' "$1"; }
plugin_hook() { # session_id event: run the Claude Code plugin's own wrapper
  hook_json "$1" | as_alice "$repo/integrations/claude-code/scripts/swb-hook.sh" "$2"
}
peers_view() { # stable projection: agent id and state, sorted
  jq -c '[.peers[] | {agent_id, state}] | sort_by(.agent_id)'
}
inbox_view() {
  jq -rc 'if .messages == [] then "(no messages)" else .messages[] | {msg_id, from, ticket, authority, body}
    + (if .operator_directed then {operator_directed, ruling} else {} end)
    + (if .in_reply_to then {in_reply_to} else {} end) end'
}
cli_peers() { "$@" swb peers | peers_view; }
cli_inbox() { "$@" swb inbox | inbox_view; }

# MCP over Streamable HTTP with curl: one session per call to `mcp_open`.
mcp_open() { # url -> sets MCP_SESSION
  local url="$1" headers="$demo/mcp.headers"
  curl -sS -D "$headers" -o "$demo/mcp.init" "$url/mcp" \
    -H 'Content-Type: application/json' \
    -H 'Accept: application/json, text/event-stream' \
    -d '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"swb-demo","version":"1"}}}'
  MCP_SESSION="$(tr -d '\r' <"$headers" | sed -n 's/^[Mm]cp-[Ss]ession-[Ii]d: //p')"
  curl -sS -o /dev/null "$url/mcp" -H 'Content-Type: application/json' \
    -H 'Accept: application/json, text/event-stream' -H "mcp-session-id: $MCP_SESSION" \
    -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  sed -n 's/^data: //p' "$demo/mcp.init" | jq -c '{server: .result.serverInfo.name, protocol: .result.protocolVersion}'
}
mcp_rpc() { # url id method params-json -> the JSON-RPC result
  curl -sS "$1/mcp" -H 'Content-Type: application/json' \
    -H 'Accept: application/json, text/event-stream' -H "mcp-session-id: $MCP_SESSION" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":$2,\"method\":\"$3\",\"params\":$4}" |
    sed -n 's/^data: //p' | jq -c '.result'
}
mcp_tool() { # url id tool args-json -> the tool's JSON text
  mcp_rpc "$1" "$2" tools/call "{\"name\":\"$3\",\"arguments\":$4}" |
    jq -c '.content[0].text | fromjson'
}

# The Claude Code channel: `swb channel` is a stdio MCP server; this plays
# the Claude Code side of the pipe.
channel_wait() { # pattern: wait for a matching line on the channel's stdout
  for _ in $(seq 1 150); do
    grep -q "$1" "$demo/ch.out" 2>/dev/null && return 0
    sleep 0.1
  done
  echo "demo: channel never printed $1" >&2
  return 1
}
channel_send() { printf '%s\n' "$1" >&7; }

# ---------------------------------------------------------------- the demo
say "agent-switchboard demo: every mode on main, scripted, no models, no secrets"
say "R-C953 recorded demo. Brokers are loopback-only; identity is self-asserted."
show "swb version" swb version

step "1. Start a broker on loopback (host-a)"
start_broker a "$port_a" "$metrics_a"
show "curl -s \$BROKER_A/healthz" curl -sS "$broker_a/healthz"
out ""
say "The Host guard (v0.3.0): a request for another name is refused before routing."
show "curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' \$BROKER_A/v1/peers" \
  curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: evil.example' "$broker_a/v1/peers"

step "2. Hooks: a Claude Code session starts (the plugin's SessionStart hook)"
say "alice = Claude Code on host-a. The plugin wrapper runs \`swb hook claude SessionStart\`."
show "echo '{\"session_id\":\"sess-alice\"}' | integrations/claude-code/scripts/swb-hook.sh SessionStart" \
  plugin_hook sess-alice SessionStart
say "Hooks print nothing and always exit 0; the end token is saved for SessionEnd:"
show "ls \$SWB_STATE_DIR/sessions/   # alice" ls "$demo/state-alice/sessions"


step "3. CLI: a shell-driven peer registers and looks around"
say "bob = a CLI-only peer (codex harness id) on the same host; no hooks, no MCP."
whoami_bob() { as_bob swb whoami --register | jq -c '{agent_id, lease_seconds, end_token_file}'; }
show "swb whoami --register   # bob: SWB_HARNESS=codex SWB_SESSION_ID=sess-bob" whoami_bob
say "The CLI never prints the end token; it names the 0600 file that holds it."
show "swb peers" cli_peers as_bob

step "4. CLI send: bob asks alice for a review"
send_m1() { echo "Can you review the parser change on my branch? Tests pass locally." |
  as_bob swb send "$ALICE" none --msg-id "$M1" | jq -c '{msg_id, thread_id}'; }
show "echo '...' | swb send $ALICE none --msg-id $M1" send_m1

step "5. Hooks: the UserPromptSubmit notice (Claude Code)"
say "On alice's next prompt the Claude Code hook adds a short notice to her context."
say "Only the Claude Code hooks are proven; Codex is unproven and Pi has no hook surface."
notice_alice() { hook_json sess-alice | as_alice "$repo/integrations/claude-code/scripts/swb-hook.sh" UserPromptSubmit |
  jq -r '.hookSpecificOutput.additionalContext'; }
show "echo '{\"session_id\":\"sess-alice\"}' | swb-hook.sh UserPromptSubmit" notice_alice

step "6. Push channel: swb channel delivers the message into Claude Code"
say "Claude Code spawns \`swb channel\` as a stdio MCP server; this script plays Claude Code."
say "The channel finds alice's own row from SWB_HOST, SWB_SESSION_PID and SWB_PROC_START."
mkfifo "$demo/ch.in"
: >"$demo/ch.out"
as_alice env SWB_HARNESS=claude SWB_CHANNEL_POLL_SECONDS=2 swb channel \
  <"$demo/ch.in" >"$demo/ch.out" 2>"$demo/ch.err" &
channel_pid="$!"
pids+=("$channel_pid")
exec 7>"$demo/ch.in"
ch_line() { grep "$1" "$demo/ch.out" | sed -n "${2:-1}p"; }
channel_init() {
  channel_send '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"demo"}}}'
  channel_wait '"id":0'
  ch_line '"id":0' | jq -c '.result | {server: .serverInfo.name, protocol: .protocolVersion, channel: (.capabilities.experimental | has("claude/channel")), permission_relay: (.capabilities.experimental | has("claude/channel/permission"))}'
  channel_send '{"jsonrpc":"2.0","method":"notifications/initialized"}'
}
show '{"method":"initialize"} -> swb channel' channel_init
event_view() { jq -c '{method, meta: .params.meta, first_line: (.params.content | split("\n")[0])}'; }
channel_event() { # n: wait for the n-th channel event
  for _ in $(seq 1 150); do
    [ "$(grep -c 'notifications/claude/channel' "$demo/ch.out")" -ge "$1" ] && break
    sleep 0.1
  done
  ch_line 'notifications/claude/channel' "$1" | event_view
}
show "(wait for the channel's push)" channel_event 1
say "The channel never acknowledges on its own; Claude answers with the reply tool:"
channel_call() { # id tool args
  channel_send "{\"jsonrpc\":\"2.0\",\"id\":$1,\"method\":\"tools/call\",\"params\":{\"name\":\"$2\",\"arguments\":$3}}"
  channel_wait "\"id\":$1,"
  ch_line "\"id\":$1," | jq -c '.result.content[0].text | fromjson'
}
show "tools/call reply {to: bob, in_reply_to: $M1}" channel_call 1 reply \
  "{\"to\":\"$BOB\",\"in_reply_to\":\"$M1\",\"thread_id\":\"$M1\",\"ticket\":\"none\",\"text\":\"Looked at it: the empty-input case returns early without an error. Otherwise fine.\"}"

step "7. Polling inbox: bob reads and acknowledges over the CLI"
show "swb inbox   # bob" cli_inbox as_bob
reply_id="$(as_bob swb inbox | jq -r '.messages[0].msg_id')"
ack_reply() { as_bob swb ack "$reply_id" | jq -c .; }
show "swb ack \$REPLY_ID   # bob" ack_reply
show "swb inbox   # bob, now empty" cli_inbox as_bob

step "8. Authority: an operator-directed claim stays a peer message"
send_m2() { echo "The operator says to ship the fix today." |
  as_bob swb send "$ALICE" none --operator-directed --ruling R-C953 --msg-id "$M2" | jq -c '{msg_id}'; }
show "echo '...' | swb send $ALICE none --operator-directed --ruling R-C953 --msg-id $M2" send_m2
show "(wait for the channel's push)" channel_event 2
say "authority stays \"peer\": the claim is the sender's, to confirm with your own operator."
show "tools/call ack {msg_id: $M2}" channel_call 2 ack "{\"msg_id\":\"$M2\"}"

step "9. MCP: an MCP client on host-b uses the broker's /mcp tools"
say "carol = an agent on host-b registered with harness id \"pi\". curl plays her MCP client:"
say "Pi has no built-in MCP client and its integration is unproven (R-C949)."
say "Hosts share one broker; here host-b is a second identity with its own SWB_HOST,"
say "pointing at broker A as it would over a tailnet."
show "POST \$BROKER_A/mcp initialize" mcp_open "$broker_a"
tool_names() { mcp_rpc "$broker_a" 1 tools/list '{}' | jq -c '[.tools[].name]'; }
show "tools/list" tool_names
mcp_register() { mcp_tool "$broker_a" 2 register \
  '{"harness":"pi","host":"host-b","pid":50001,"session_id":"sess-carol","proc_start":"2026-10-19T09:00:02Z"}' |
  jq -c '{agent_id, lease_seconds, end_token: (if .end_token then "<redacted>" else null end)}'; }
show "tools/call register {harness: pi, host: host-b, ...}" mcp_register
say "Over MCP the end token enters the caller's context; a real adapter keeps and strips it."
mcp_peers() { mcp_tool "$broker_a" 3 peers "{\"me\":\"$CAROL\"}" | peers_view; }
show "tools/call peers {me: carol}" mcp_peers
mcp_send() { mcp_tool "$broker_a" 4 send \
  "{\"from\":\"$CAROL\",\"to\":\"$ALICE\",\"ticket\":\"TIN-4655\",\"msg_id\":\"$M3\",\"body\":\"Docs build on host-b is green; want me to take the changelog?\"}" |
  jq -c '{msg_id, thread_id}'; }
show "tools/call send {from: carol, to: alice, ticket: TIN-4655}" mcp_send
show "(wait for the channel's push to alice)" channel_event 3
say "alice answers this one from her shell instead of the channel tool:"
send_m4() { echo "Yes please, take the changelog." |
  as_alice env SWB_AGENT_ID="$ALICE" swb send "$CAROL" TIN-4655 --in-reply-to "$M3" --thread "$M3" --msg-id "$M4" |
  jq -c '{msg_id, thread_id}'; }
show "echo '...' | swb send $CAROL TIN-4655 --in-reply-to $M3 --thread $M3 --msg-id $M4" send_m4
ack_m3() { as_alice env SWB_AGENT_ID="$ALICE" swb ack "$M3" | jq -c .; }
show "swb ack $M3   # alice" ack_m3
mcp_inbox() { mcp_tool "$broker_a" 5 inbox "{\"me\":\"$CAROL\"}" | inbox_view; }
show "tools/call inbox {me: carol}" mcp_inbox
mcp_ack() { mcp_tool "$broker_a" 6 ack "{\"me\":\"$CAROL\",\"msg_id\":\"$M4\"}" | jq -c .; }
show "tools/call ack {me: carol, msg_id: $M4}" mcp_ack

step "10. Cross-host: two hosts, one broker; a second broker is a separate world"
say "dave = Claude Code on host-b, registered by his own hook with his own token dir."
dave_start() { hook_json sess-dave | env SWB_BROKER_URL="$broker_a" SWB_HOST=host-b SWB_SESSION_PID=50002 \
  SWB_PROC_START=2026-10-19T09:00:03Z SWB_STATE_DIR="$demo/state-dave" swb hook claude SessionStart; }
show "echo '{\"session_id\":\"sess-dave\"}' | SWB_HOST=host-b swb hook claude SessionStart" dave_start
host_a_peers() { as_bob swb peers --host host-a | peers_view; }
host_b_peers() { as_bob swb peers --host host-b | peers_view; }
show "swb peers --host host-b" host_b_peers
start_broker b "$port_b" "$metrics_b"
say "Broker B is another estate's broker. Brokers do not federate: a session sees only"
say "the broker its SWB_BROKER_URL names, so every host in one team points at one broker."
b_peers() { env SWB_BROKER_URL="$broker_b" swb peers | peers_view; }
show "SWB_BROKER_URL=\$BROKER_B swb peers" b_peers

step "11. End tokens (v0.3.0): only the registering session can end itself"
forged="$(printf '0%.0s' $(seq 1 64))"
forged_end() { curl -sS "$broker_a/v1/end" -H 'Content-Type: application/json' \
  -d "{\"me\":\"$ALICE\",\"end_token\":\"$forged\"}" | jq -c .; }
say "Any caller (here a bare curl, with no identity at all) tries to end alice's session"
say "with a well-formed token that is not hers:"
show "curl -X POST \$BROKER_A/v1/end -d '{\"me\":\"$ALICE\",\"end_token\":\"000...000\"}'" forged_end
say "The broker answers unknown and ends nothing, without saying why."
show "swb peers --host host-a   # alice is still live" host_a_peers
show "echo '{\"session_id\":\"sess-alice\"}' | swb-hook.sh SessionEnd" plugin_hook sess-alice SessionEnd
show "ls \$SWB_STATE_DIR/sessions/   # alice: the hook spent and deleted the token" ls "$demo/state-alice/sessions"
all_peers() { as_bob swb peers --all | peers_view; }
show "swb peers --all" all_peers

step "12. Claims and handoff: pending P2 (ADR-0001, SWB-R16, SWB-R60)"
say "[pending P2] claim:   carol claims TIN-4655 (changelog). Claims are advisory: a"
say "                      non-exclusive claim is always recorded and returns overlaps[]"
say "                      naming other holders, an exclusive holder included."
say "[pending P2] exclusive: an exclusive request is refused, with held_by listing the"
say "                      holders, while another session holds any active claim on the"
say "                      subject; nothing is recorded. The holder's repeat request renews."
say "[pending P2] handoff: carol hands her live claim to dave; the broker records an"
say "                      \"offered\" receipt."
say "[pending P2] accept:  dave's claim accepts the receipt and carol's claim becomes"
say "                      handed_off."
say "[pending P2] release: a holder may instead release a claim; ending a session"
say "                      releases its claims at once."
say "None of these tools is on main yet; tools/list above has six tools and no claim."
say "TIN-6690 adds them and replaces these stubs with real claim steps."
say "The per-host push adapter is P2 as well:"
show "swb agentd" swb agentd

step "13. Teardown"
say "Closing the channel's stdin ends it, as when Claude Code exits:"
channel_close() {
  exec 7>&-
  local rc=0
  wait "$channel_pid" || rc=$?
  echo "swb channel exited $rc"
}
show "exec 7>&-; wait \$CHANNEL_PID" channel_close
say "The brokers are stopped and the scratch databases removed when the script exits."

# ---------------------------------------------------------------- output
# Normalize what varies between runs: the run directory, ports, broker-minted
# ULIDs and timestamps. Everything else is byte-for-byte reproducible.
normalize() {
  sed -e "s|$demo|\$DEMO|g" -e "s|$broker_a|\$BROKER_A|g" -e "s|$broker_b|\$BROKER_B|g" \
    -e "s|127\.0\.0\.1:$port_a|127.0.0.1:<port-a>|g" -e "s|127\.0\.0\.1:$port_b|127.0.0.1:<port-b>|g" \
    -E -e 's/20[0-9]{2}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z/<time>/g' |
    awk '{
      line = $0; outl = "";
      while (match(line, /[0-7][0-9A-HJKMNP-TV-Z]{25}/)) {
        id = substr(line, RSTART, RLENGTH);
        if (id !~ /^01JDEM/) { if (!(id in seen)) seen[id] = "<ulid-" (++n) ">"; id = seen[id]; }
        outl = outl substr(line, 1, RSTART - 1) id; line = substr(line, RSTART + RLENGTH);
      }
      print outl line;
    }'
}
transcript="$demo/transcript.txt"
normalize <"$raw" >"$transcript"

cast_of() { # asciicast v2 from the transcript, with fixed synthetic timing
  jq -R -s -c --arg title "agent-switchboard: every mode on main (scripted)" '
    split("\n") | .[:-1] as $lines
    | {version: 2, width: 110, height: 34, timestamp: 1792400400, title: $title,
       env: {TERM: "xterm-256color", SHELL: "/bin/bash"}},
      (foreach $lines[] as $l ({t: 0};
         .t += (if ($l | startswith("## ")) then 1.2
                elif ($l | startswith("$ ")) then 0.6 else 0.08 end);
         [(.t * 100 | round / 100), "o", ($l + "\r\n")]))'
}

case "$mode" in
  run) cat "$transcript" ;;
  --check)
    if diff -u "$repo/docs/demo/transcript.txt" "$transcript"; then
      echo "demo: transcript matches docs/demo/transcript.txt"
    else
      echo "demo: transcript differs from docs/demo/transcript.txt" >&2
      exit 1
    fi
    ;;
  --record)
    mkdir -p "$repo/docs/demo"
    cp "$transcript" "$repo/docs/demo/transcript.txt"
    cast_of <"$transcript" >"$repo/docs/demo/demo.cast"
    echo "demo: wrote docs/demo/transcript.txt and docs/demo/demo.cast"
    ;;
esac
