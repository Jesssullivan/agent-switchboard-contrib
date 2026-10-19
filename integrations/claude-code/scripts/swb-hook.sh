#!/bin/sh
# Fail-open Claude Code hook wrapper for `swb hook claude <event>`.
#
# Contract (SWB-R10): this script always exits 0 and never blocks the
# session. A missing `swb`, an unset SWB_BROKER_URL, a slow or unreachable
# broker, or any `swb` failure results in no output and exit 0. Claude Code
# treats exit 2 as "block" for UserPromptSubmit and Stop, so this script
# never returns it.
#
# Bounds, innermost first:
#   1. `swb hook` bounds its own stdin read and broker calls (1.8 s).
#   2. `timeout 3` here, when a `timeout` command is on PATH.
#   3. The hook's "timeout": 3 in hooks.json, enforced by Claude Code.
#
# stdin (the hook JSON with session_id) is passed through to `swb` unread.
# stdout is passed through, so a UserPromptSubmit notice reaches Claude.

event="${1:-}"
case "$event" in
  SessionStart | UserPromptSubmit | Stop | SessionEnd) ;;
  *) exit 0 ;;
esac

command -v swb >/dev/null 2>&1 || exit 0
[ -n "${SWB_BROKER_URL:-}" ] || exit 0

if command -v timeout >/dev/null 2>&1; then
  timeout 3 swb hook claude "$event" 2>/dev/null || true
else
  swb hook claude "$event" 2>/dev/null || true
fi
exit 0
