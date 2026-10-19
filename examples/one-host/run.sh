#!/usr/bin/env bash
# Two sessions on one host: Claude Code and a CLI-driven peer, through a
# loopback broker this script starts (R-C953, TIN-6694).
#
#   run.sh up                 start a loopback broker and write env.sh
#   run.sh claude [args...]   exec Claude Code with this session's SWB_* identity,
#                             the plugin and the channel (you run this, in a terminal)
#   run.sh peer register      register the CLI peer (prints its agent id)
#   run.sh peer peers         list live sessions
#   run.sh peer send <to> [ticket]   send stdin to <to> (ticket: TIN-<digits> or none)
#   run.sh peer inbox         read the peer's unacknowledged mail
#   run.sh peer ack <msg_id>  acknowledge one message
#   run.sh peer end           end the peer's session (spends its end token)
#   run.sh down               stop the broker and delete its state
#   run.sh selftest           the whole flow with a scripted Claude side; exits 0 on success
#
# Needs an `swb` 0.3.x binary (main) on PATH or in SWB_BIN, plus curl and jq.
# The broker binds 127.0.0.1 only. Identity is self-asserted: anyone who can
# reach the port can act as any agent id, so keep it on loopback.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
swb="${SWB_BIN:-$(command -v swb || true)}"
[ -x "$swb" ] || { echo "one-host: no swb binary (set SWB_BIN)" >&2; exit 3; }

port="${SWB_EXAMPLE_PORT:-18280}"
metrics="${SWB_EXAMPLE_METRICS_PORT:-19280}"
dir="${SWB_EXAMPLE_DIR:-${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/swb-one-host}"
host="${SWB_HOST:-$(hostname -s | tr -c 'A-Za-z0-9._\n-' '-')}"
peer_harness="${SWB_EXAMPLE_PEER_HARNESS:-codex}"
peer_session="${SWB_EXAMPLE_PEER_SESSION:-cli-peer}"
peer_pid=40999

# True only when <pid> is still a `swb serve` process: a pid file can outlive
# its broker, and the kernel may have reused the number for something else.
is_our_broker() {
  local pid="$1" args
  kill -0 "$pid" 2>/dev/null || return 1
  args="$(ps -o args= -p "$pid" 2>/dev/null)" || return 1
  case "$args" in *swb*" serve"*) return 0 ;; *) return 1 ;; esac
}

port_free() { ! curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$1/" 2>/dev/null; }

up() {
  mkdir -p "$dir"
  chmod 0700 "$dir"
  if [ -f "$dir/broker.pid" ] && is_our_broker "$(cat "$dir/broker.pid")"; then
    echo "one-host: broker already running (pid $(cat "$dir/broker.pid"))"
  else
    for p in "$port" "$metrics"; do
      port_free "$p" || {
        echo "one-host: 127.0.0.1:$p is already in use; set SWB_EXAMPLE_PORT and SWB_EXAMPLE_METRICS_PORT" >&2
        exit 3
      }
    done
    env SWB_DB_PATH="$dir/swb.sqlite3" SWB_LISTEN="127.0.0.1:$port" \
      SWB_METRICS_LISTEN="127.0.0.1:$metrics" nohup "$swb" serve >"$dir/broker.log" 2>&1 &
    local pid="$!" ok=""
    echo "$pid" >"$dir/broker.pid"
    # Healthy only while our own process is alive, so a foreign listener on
    # the same port can never pass for this broker.
    for _ in $(seq 1 100); do
      kill -0 "$pid" 2>/dev/null || break
      if curl -sf "http://127.0.0.1:$metrics/healthz" >/dev/null 2>&1 &&
        curl -sf "http://127.0.0.1:$port/healthz" >/dev/null 2>&1 &&
        kill -0 "$pid" 2>/dev/null; then
        ok=1
        break
      fi
      sleep 0.1
    done
    [ -n "$ok" ] || { echo "one-host: broker did not start; see $dir/broker.log" >&2; exit 1; }
    echo "one-host: broker on http://127.0.0.1:$port (pid $pid)"
  fi
  cat >"$dir/env.sh" <<EOF
export SWB_BROKER_URL=http://127.0.0.1:$port
export SWB_METRICS_URL=http://127.0.0.1:$metrics
export SWB_HOST=$host
EOF
  echo "one-host: wrote $dir/env.sh"
}

down() {
  if [ -f "$dir/broker.pid" ]; then
    local pid
    pid="$(cat "$dir/broker.pid")"
    if is_our_broker "$pid"; then
      kill "$pid" 2>/dev/null || true
    else
      echo "one-host: pid $pid is not this broker any more; not signalling it" >&2
    fi
  fi
  rm -rf "$dir"
  echo "one-host: broker stopped, $dir removed"
}

load_env() {
  [ -f "$dir/env.sh" ] || { echo "one-host: run '$0 up' first" >&2; exit 3; }
  # shellcheck source=/dev/null
  . "$dir/env.sh"
}

# The Claude Code side. A launcher exports the session identity from its own
# pid and then execs the harness, so the pid stays the session's pid.
claude_session() {
  load_env
  export SWB_SESSION_PID=$$
  SWB_PROC_START="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  export SWB_PROC_START SWB_STATE_DIR="$dir/state-claude"
  # Channel push needs the development flag during the channels research
  # preview; see integrations/claude-code/README.md. Without it the hooks'
  # UserPromptSubmit notice still tells the session about new mail.
  # The bare-server form: the channel from .mcp.json and the hooks from the
  # settings example, both of which call `swb` from PATH.
  mkdir -p "$dir/bin"
  ln -sf "$swb" "$dir/bin/swb"
  export PATH="$dir/bin:$PATH"
  exec claude --mcp-config "$repo/integrations/claude-code/.mcp.json" \
    --settings "$repo/integrations/claude-code/examples/settings.hooks.json" \
    --dangerously-load-development-channels server:swb-channel "$@"
}

peer_env() {
  load_env
  export SWB_HARNESS="$peer_harness" SWB_SESSION_ID="$peer_session" \
    SWB_SESSION_PID="$peer_pid" SWB_PROC_START=cli-peer SWB_STATE_DIR="$dir/state-peer" \
    SWB_AGENT_ID="$peer_harness:$host:$peer_pid:$peer_session"
}

peer() {
  local verb="${1:-}"
  shift || true
  peer_env
  case "$verb" in
    register) swb_run whoami --register ;;
    peers) swb_run peers ;;
    send) [ $# -ge 1 ] || { echo "usage: $0 peer send <to> [ticket] < body" >&2; exit 2; }
      swb_run send "$1" "${2:-none}" ;;
    inbox) swb_run inbox ;;
    ack) [ $# -eq 1 ] || { echo "usage: $0 peer ack <msg_id>" >&2; exit 2; }
      swb_run ack "$1" ;;
    end) printf '{"session_id":"%s"}' "$peer_session" | "$swb" hook "$peer_harness" SessionEnd
      echo "one-host: peer session ended" ;;
    *) echo "usage: $0 peer register|peers|send|inbox|ack|end" >&2; exit 2 ;;
  esac
}
swb_run() { "$swb" "$@"; }

# Scripted end-to-end check: the Claude side is played by the same hooks
# Claude Code would run, so no model is called.
selftest() {
  # Global, not local: the EXIT trap runs after this function returns.
  selftest_scratch="$(mktemp -d "${TMPDIR:-/tmp}/swb-one-host-test.XXXXXX")"
  export SWB_EXAMPLE_DIR="$selftest_scratch/run"
  dir="$SWB_EXAMPLE_DIR"
  trap 'down >/dev/null; rm -rf "$selftest_scratch"' EXIT
  up >/dev/null
  load_env
  local claude_id="claude:$host:41000:sess-claude"
  claude_hook() {
    printf '{"session_id":"sess-claude"}' |
      env SWB_SESSION_PID=41000 SWB_PROC_START=selftest SWB_STATE_DIR="$dir/state-claude" \
        PATH="$dir/bin:$PATH" "$repo/integrations/claude-code/scripts/swb-hook.sh" "$1"
  }
  mkdir -p "$dir/bin"
  ln -sf "$swb" "$dir/bin/swb"
  claude_hook SessionStart
  local peer_id
  peer_id="$(peer register | jq -r .agent_id)"
  [ "$peer_id" = "$peer_harness:$host:$peer_pid:$peer_session" ]
  peer peers | jq -e --arg c "$claude_id" --arg p "$peer_id" \
    '[.peers[].agent_id] | index($c) and index($p)' >/dev/null
  echo "selftest: hello from the CLI peer" | peer send "$claude_id" none >/dev/null
  claude_hook UserPromptSubmit | jq -e '.hookSpecificOutput.additionalContext | contains("Unread peer message")' >/dev/null
  local msg
  msg="$(SWB_AGENT_ID="$claude_id" "$swb" inbox | jq -r '.messages[0].msg_id')"
  echo "selftest: hello back" | SWB_AGENT_ID="$claude_id" "$swb" send "$peer_id" none --in-reply-to "$msg" >/dev/null
  SWB_AGENT_ID="$claude_id" "$swb" ack "$msg" >/dev/null
  peer inbox | jq -e '.messages[0].body == "selftest: hello back"' >/dev/null
  peer ack "$(peer inbox | jq -r '.messages[0].msg_id')" >/dev/null
  peer inbox | jq -e '.messages == []' >/dev/null
  # End both sessions and prove it: hooks exit 0 silently even on failure,
  # so check the token files are spent and the broker shows both ended.
  ls "$dir/state-peer/sessions/"* >/dev/null 2>&1
  ls "$dir/state-claude/sessions/"* >/dev/null 2>&1
  peer end >/dev/null
  claude_hook SessionEnd
  # (`! cmd` never trips set -e, hence the explicit test.)
  for d in "$dir/state-peer/sessions" "$dir/state-claude/sessions"; do
    if ls "$d/"* >/dev/null 2>&1; then
      echo "one-host selftest: an end token in $d was not spent" >&2
      exit 1
    fi
  done
  "$swb" peers --all | jq -e --arg c "$claude_id" --arg p "$peer_id" \
    '[.peers[] | select(.agent_id == $c or .agent_id == $p) | .state] == ["ended", "ended"]' >/dev/null
  "$swb" peers | jq -e --arg c "$claude_id" --arg p "$peer_id" \
    '[.peers[].agent_id] | (index($c) | not) and (index($p) | not)' >/dev/null
  echo "one-host selftest: ok (register, peers, send, notice, inbox, reply, ack, end, tokens spent)"
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  claude) shift; claude_session "$@" ;;
  peer) shift; peer "$@" ;;
  selftest) selftest ;;
  *) sed -n '2,20p' "$0"; exit 2 ;;
esac
