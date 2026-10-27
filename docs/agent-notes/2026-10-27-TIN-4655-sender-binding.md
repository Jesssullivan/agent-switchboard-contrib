---
title: "Opt-in sender binding, send rate limit and stable bound delivery (ADR-0003)"
date: 2026-10-27
status: active
summary: >-
  Implements ADR-0003 under SWB-R66 to SWB-R68 for v0.5.0: session tokens
  for bound ids, rotation on every register, a 0600 client token file, a
  per-sender 429 limit and inbox delivery keyed without the pid.
refs:
  - TIN-4655
---

# Opt-in sender binding (ADR-0003)

Authority: operator rulings SWB-R66, SWB-R67 and SWB-R68 (Linear TIN-4655
comment `0753094f`, 2026-10-27), and R-HOOK-CONVERGENCE-20261004 (R-N13
receipt). Branch `feat/tin-4655-sender-binding-20261027` on the fork,
from upstream main `00867d18`.

## Change

| Area | What changed |
| --- | --- |
| Store | `sessions.bound`, `messages.recipient_key` (filled in place), `session_token_key`; `register` takes `bind` and `session_token`; `authorize`; key-based `inbox` and `ack` for a bound id; `bound` on `peers` rows |
| Broker | 401 `unauthorized` for a bound id without its token on `send`, `inbox`, `ack`, `claim`, `release`, `handoff`, `end`; 429 `rate_limited` from a per-sender bucket; token stripped before the store and audit; MCP schemas gain `bind` and `session_token` |
| Client | `SWB_BIND=1`; token file `$SWB_STATE_DIR/bindings/<harness>:<host>:<session_id>.token` (0700 dir, 0600 file), rotated on every register; acting verbs, SessionEnd, `doctor --register` and `swb channel` send it |
| Docs | ADR-0003 accepted; ADR-0001 SWB-R05 and SWB-R14 amended, rows SWB-R66 to SWB-R68; SECURITY, REFERENCE, CHANGELOG `[Unreleased]`, AGENTS invariants |
| Spec | `Broker.dhall` `binding` block and regenerated constants; spec live runs set `SWB_SEND_BURST=0` |

## Choices the ruling left open

- Bucket size and refill: burst 20, refill 30 per minute, set by
  `SWB_SEND_BURST` and `SWB_SEND_REFILL_PER_MINUTE`; `0` burst turns it off.
  The limit applies to every sender, after the binding check.
- Token storage: SWB-R67 fixes the 0600 file. The path is keyed without
  the pid so a restart finds it.
- Takeover: a new register of a live bound key needs the current token.
  Without that rule a spoofer could register the key with `bind: true` and
  get a fresh token, which would defeat the spoof test in practice.
- Lapse: a binding ends with its session or after 6 hours without a call
  (the `gone` band). This is the recovery path for a lost token file.

These are implementation choices inside the accepted ADR. A reviewer may
want a ruling on the takeover rule, the lapse window, and the rate limit
applying to unbound senders.

## Evidence

All runs on sting under `systemd-run --user --scope -p MemoryMax=6G -p
MemorySwapMax=0 nice -n 19`, with `CARGO_BUILD_BUILD_DIR` unset and
`CARGO_TARGET_DIR` inside the worktree. Bazel ran with `--nohome_rc`, so
neither the repository cache nor the disk cache under `/srv/cache/jess` was
used.

| Gate | Result |
| --- | --- |
| `bazelisk test --lockfile_mode=error --remote_local_fallback=false //:check` (the `just check` command, local and non-qualifying) | 11 of 11 targets pass at `bdd4351` |
| `cargo test --workspace --locked` (flake dev shell, rust 1.97.1) | 151 passed: swb 60, broker 35 plus 1, store 52, proto 2, agentd 1 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | clean |
| `just spec-dhall` | ok |
| `nix build .#swb-spec` (the `just spec-quickcheck` command) | 7 properties, 1000 cases each, all pass |
| gitleaks (full history, `.gitleaks.toml`) | 193 commits, no leaks |
| TruffleHog `--only-verified`, branch commits since `00867d18` | 0 verified, 0 unverified |

Acceptance tests named by the ruling:

- Spoof test: `spoofed_send_as_a_bound_id_fails_over_rest_and_mcp` (broker)
  and `a_bound_registration_needs_its_session_token_and_rotates_it` (store).
- Rotation: `a_bound_client_rotates_its_token_on_every_register` (broker),
  `a_bound_client_keeps_its_session_token_in_a_0600_file_and_rotates_it`
  (CLI).
- Restart keeps mail: `bound_service_restart_keeps_unread_mail_through_a_broker_reopen`
  (production service, file reopen) and
  `a_restarted_bound_service_keeps_its_unread_mail` (store).
- Rate limit 429: `an_empty_send_bucket_answers_429_rate_limited`.
- Unbound compatibility: `unbound_sessions_work_as_in_v040` (broker) and
  `unbound_ids_behave_as_in_v040` (store).

## Next

- Upstream review of the PR; no merge, tag or release from this lane.
- A version bump to 0.5.0 and a release record follow the v0.4.0 pattern
  (#51, #52) under their own ruling.
- Cade registers bound (X02) once a v0.5.0 broker is live.
