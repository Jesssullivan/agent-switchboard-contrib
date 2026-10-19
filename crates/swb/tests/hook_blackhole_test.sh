#!/usr/bin/env bash
# SWB-R10 / R-C949 (TIN-6688): a harness hook against a blackholed broker
# returns within 2 s and exits 0, with no output.
#
# The broker address is 192.0.2.1:9 (RFC 5737 TEST-NET-1, never routed), so
# a SYN gets no answer and connect() waits until the client's own deadline.
# Where the sandbox has no route at all, connect fails at once instead; the
# assertions hold either way and the printed elapsed times show which path
# ran. A second case holds stdin open with no EOF, which deterministically
# exercises the hook's own bound on its stdin read.
#
# Cases: the compiled `swb hook claude <event>` directly, and the Claude Code
# wrapper `integrations/claude-code/scripts/swb-hook.sh`, for every event.
set -euo pipefail

swb="$1"
wrapper="$2"
[ -x "$swb" ] || { echo "not executable: $swb" >&2; exit 1; }
[ -f "$wrapper" ] || { echo "missing wrapper: $wrapper" >&2; exit 1; }

budget_us=2000000
scratch="${TEST_TMPDIR:-$(mktemp -d)}/hook-blackhole"
mkdir -p "$scratch/bin" "$scratch/state"
chmod 700 "$scratch/state"
ln -sf "$(cd "$(dirname "$swb")" && pwd)/$(basename "$swb")" "$scratch/bin/swb"

export SWB_BROKER_URL="http://192.0.2.1:9"
export SWB_HOST="blackhole-test"
export SWB_SESSION_PID="4242"
export SWB_PROC_START="blackhole-test-start"
export SWB_STATE_DIR="$scratch/state"
unset SWB_HARNESS SWB_AGENT_ID SWB_SESSION_ID

# SessionEnd reaches the broker only with a saved end token, so seed one.
mkdir -p "$scratch/state/sessions"
chmod 700 "$scratch/state/sessions"
token_file="$scratch/state/sessions/claude:blackhole-test:4242:blackhole-session.token"
printf '%064d' 0 >"$token_file"
chmod 600 "$token_file"

now_us() {
  if [ -n "${EPOCHREALTIME:-}" ]; then
    local t="${EPOCHREALTIME/[.,]/}"
    echo "$t"
  else
    echo $(($(date +%s%N) / 1000))
  fi
}

failures=0
run_case() {
  local name="$1" stdin_mode="$2"
  shift 2
  local out="$scratch/out" start end elapsed status=0
  start="$(now_us)"
  if [ "$stdin_mode" = "open" ]; then
    # A writer that never writes or closes: the hook sees no EOF.
    local fifo="$scratch/fifo"
    rm -f "$fifo"
    mkfifo "$fifo"
    sleep 30 >"$fifo" &
    local holder=$!
    "$@" <"$fifo" >"$out" 2>/dev/null || status=$?
    kill "$holder" 2>/dev/null || true
    wait "$holder" 2>/dev/null || true
  else
    printf '%s' '{"session_id":"blackhole-session","hook_event_name":"x"}' \
      | "$@" >"$out" 2>/dev/null || status=$?
  fi
  end="$(now_us)"
  elapsed=$((end - start))
  local verdict=ok
  if [ "$status" -ne 0 ] || [ "$elapsed" -ge "$budget_us" ] || [ -s "$out" ]; then
    verdict=FAIL
    failures=$((failures + 1))
  fi
  printf '%-4s %-48s exit=%d elapsed_ms=%d stdout_bytes=%d\n' \
    "$verdict" "$name" "$status" $((elapsed / 1000)) "$(wc -c <"$out")"
}

for event in SessionStart UserPromptSubmit Stop SessionEnd; do
  run_case "swb hook claude $event" closed "$swb" hook claude "$event"
  run_case "swb-hook.sh $event" closed env PATH="$scratch/bin:$PATH" sh "$wrapper" "$event"
done
run_case "swb hook claude SessionStart (stdin never closes)" open "$swb" hook claude SessionStart
run_case "swb-hook.sh UserPromptSubmit (stdin never closes)" open \
  env PATH="$scratch/bin:$PATH" sh "$wrapper" UserPromptSubmit

# The hook must not have consumed the seeded token on a failed end.
[ -f "$token_file" ] || { echo "FAIL seeded end token was removed without a broker answer"; failures=$((failures + 1)); }

if [ "$failures" -ne 0 ]; then
  echo "$failures case(s) exceeded the 2 s budget, exited non-zero or printed output" >&2
  exit 1
fi
echo "all hook cases returned exit 0 within 2 s against a blackholed broker"
