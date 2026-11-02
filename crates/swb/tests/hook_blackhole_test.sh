#!/usr/bin/env bash
# SWB-R10 / R-C949 (TIN-6688): a harness hook against a blackholed broker
# returns within 4 s and exits 0, with no output. SWB-R10's budget was 2 s;
# it is 4 s since the hook bound rose to 3.5 s (TIN-7577 comment 2f1439dc,
# and Jess, "Yes, 4 second timeout (Recommended)", TIN-4655 comment
# 204ebf82).
#
# The broker address is 192.0.2.1:9 (RFC 5737 TEST-NET-1, never routed), so
# a SYN gets no answer and connect() waits until the client's own deadline.
# Where the sandbox has no route at all, connect fails at once instead. The
# exit/output/budget assertions hold either way, but that run never reaches
# the blackhole path, so it ends with a NO-ROUTE line instead of the pass
# line: the fixture driver (scripts/e2e-fixtures.py) matches only the pass
# line and so does not count a NO-ROUTE run as blackhole evidence. A second
# case holds stdin open with no EOF, which deterministically exercises the
# hook's own bound on its stdin read.
#
# Timing margin: the SWB-R10 ceiling is 4 s and the hook's own bound is
# HOOK_LIMIT = 3500 ms (crates/swb/src/main.rs), leaving about 500 ms for
# fork/exec of env, sh and swb (66 ms over the bound was the worst case
# measured on sting with the 1800 ms bound). A case that exits 0 with no
# output but overshoots 4 s is run once more, and a second overshoot fails.
# That retry also covers a hook that hangs for a bounded time and then
# exits 0 (say at 4.5 s): it fails only if it overshoots twice. A case that exits
# non-zero or prints fails on its first run. There is no per-case timeout: a
# hook that never returns is caught by Bazel's timeout for the whole target
# (size "small", 60 s) as a TIMEOUT, not as a FAIL line for that case.
#
# NO-ROUTE: with SWB_HOOK_REQUIRE_BLACKHOLE=1 (set by the fixture driver) a
# run where no case reached the blackhole fails. Without it (CI) the run
# still checks exit, output and budget, and prints NO-ROUTE, so a green CI
# run is not by itself evidence that the blackhole path ran.
#
# Cases: the compiled `swb hook claude <event>` directly, and the Claude Code
# wrapper `integrations/claude-code/scripts/swb-hook.sh`, for every event.
# Arguments after the second (for example the fixture driver's
# --test-threads=1, which Bazel passes to every test) are ignored.
#
# Bound cases (review K1 of #57): with a saved session token under
# `bindings/`, a register that misses its 1.5 s deadline is sent once more,
# so against the blackhole the hook waits for two deadlines, about 3 s. A
# bound case that waited on connect must last at least 2.5 s and stay under
# the 4 s budget. That fails when the hook bound or the wrapper's `timeout`
# backstop cuts the retry short, as the former `timeout 2` did at about 2 s.
# A bound case that returned at once is a NO-ROUTE run, as above.
set -euo pipefail

swb="$1"
wrapper="$2"
[ -x "$swb" ] || { echo "not executable: $swb" >&2; exit 1; }
[ -f "$wrapper" ] || { echo "missing wrapper: $wrapper" >&2; exit 1; }

budget_us=4000000
# A closed-stdin case that reached the blackhole waits on connect for most of
# HOOK_LIMIT; one that failed at once (no route) returns in a few ms.
blackhole_min_us=1000000
# A bound case that reached the blackhole waits for the register and its one
# retry, two 1.5 s deadlines; one cut at 2 s is short of this.
bound_min_us=2500000
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
    # bash 3.2 (macOS) has no EPOCHREALTIME and BSD date has no %N.
    perl -MTime::HiRes=time -e 'printf "%d\n", time() * 1000000'
  fi
}

failures=0
blackholed=0
# Runs one case; sets case_status, case_elapsed and case_bytes.
attempt() {
  local stdin_mode="$1"
  shift
  local out="$scratch/out" start end
  case_status=0
  start="$(now_us)"
  if [ "$stdin_mode" = "open" ]; then
    # A writer that never writes or closes: the hook sees no EOF.
    local fifo="$scratch/fifo"
    rm -f "$fifo"
    mkfifo "$fifo"
    sleep 30 >"$fifo" &
    local holder=$!
    "$@" <"$fifo" >"$out" 2>/dev/null || case_status=$?
    kill "$holder" 2>/dev/null || true
    wait "$holder" 2>/dev/null || true
  else
    printf '%s' '{"session_id":"blackhole-session","hook_event_name":"x"}' \
      | "$@" >"$out" 2>/dev/null || case_status=$?
  fi
  end="$(now_us)"
  case_elapsed=$((end - start))
  case_bytes="$(wc -c <"$out" | tr -d ' ')"
}

run_case() {
  local name="$1" stdin_mode="$2" min_us=0
  shift 2
  if [ "$stdin_mode" = "bound" ]; then
    stdin_mode=closed
    min_us="$bound_min_us"
  fi
  local verdict=ok
  attempt "$stdin_mode" "$@"
  if [ "$case_status" -eq 0 ] && [ "$case_bytes" -eq 0 ] && [ "$case_elapsed" -ge "$budget_us" ]; then
    printf 'RETRY %-47s exit=%d elapsed_ms=%d (timing only; once)\n' \
      "$name" "$case_status" $((case_elapsed / 1000))
    attempt "$stdin_mode" "$@"
  fi
  if [ "$case_status" -ne 0 ] || [ "$case_elapsed" -ge "$budget_us" ] || [ "$case_bytes" -ne 0 ]; then
    verdict=FAIL
    failures=$((failures + 1))
  elif [ "$case_elapsed" -ge "$blackhole_min_us" ] && [ "$case_elapsed" -lt "$min_us" ]; then
    # It reached the blackhole, but the retry did not get its deadline.
    verdict=FAIL
    failures=$((failures + 1))
  fi
  if [ "$stdin_mode" = "closed" ] && [ "$case_elapsed" -ge "$blackhole_min_us" ]; then
    blackholed=$((blackholed + 1))
  fi
  printf '%-5s %-47s exit=%d elapsed_ms=%d margin_ms=%d stdout_bytes=%d\n' \
    "$verdict" "$name" "$case_status" $((case_elapsed / 1000)) \
    $(((budget_us - case_elapsed) / 1000)) "$case_bytes"
}

for event in SessionStart UserPromptSubmit Stop SessionEnd; do
  run_case "swb hook claude $event" closed "$swb" hook claude "$event"
  run_case "swb-hook.sh $event" closed env PATH="$scratch/bin:$PATH" sh "$wrapper" "$event"
done
run_case "swb hook claude SessionStart (stdin never closes)" open "$swb" hook claude SessionStart
run_case "swb-hook.sh UserPromptSubmit (stdin never closes)" open \
  env PATH="$scratch/bin:$PATH" sh "$wrapper" UserPromptSubmit

# Bound cases: seed a session token, so each register is retried once.
mkdir -p "$scratch/state/bindings"
chmod 700 "$scratch/state/bindings"
binding_file="$scratch/state/bindings/claude:blackhole-test:blackhole-session.token"
printf '%064d' 1 >"$binding_file"
chmod 600 "$binding_file"
run_case "swb hook claude UserPromptSubmit (bound, retried)" bound \
  "$swb" hook claude UserPromptSubmit
run_case "swb-hook.sh UserPromptSubmit (bound, retried)" bound \
  env PATH="$scratch/bin:$PATH" sh "$wrapper" UserPromptSubmit
# The hook must not have dropped the session token without a broker answer.
[ -f "$binding_file" ] || { echo "FAIL seeded session token was removed without a broker answer"; failures=$((failures + 1)); }

# The hook must not have consumed the seeded token on a failed end.
[ -f "$token_file" ] || { echo "FAIL seeded end token was removed without a broker answer"; failures=$((failures + 1)); }

if [ "$failures" -ne 0 ]; then
  echo "$failures case(s) exceeded the 4 s budget, exited non-zero, printed output or cut a bound retry short" >&2
  exit 1
fi
if [ "$blackholed" -eq 0 ]; then
  # Every closed-stdin case returned in under 1 s: connect failed at once, so
  # the blackhole path did not run. Not a contract break, and not evidence.
  echo "NO-ROUTE: all hook cases returned exit 0 within 4 s, but no closed-stdin case waited on connect; the blackhole path did not run"
  if [ "${SWB_HOOK_REQUIRE_BLACKHOLE:-0}" = "1" ]; then
    echo "SWB_HOOK_REQUIRE_BLACKHOLE=1: a NO-ROUTE run is not accepted" >&2
    exit 1
  fi
  exit 0
fi
echo "all hook cases returned exit 0 within 4 s against a blackholed broker ($blackholed closed-stdin cases waited on connect)"
