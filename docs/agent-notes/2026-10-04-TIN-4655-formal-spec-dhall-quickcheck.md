---
title: "Formal spec: Dhall records and QuickCheck model (R-C229)"
date: 2026-10-04
status: active
summary: >-
  Adds spec/ with Dhall types and records for approved-broker.json and blahaj
  owners.json, ruled broker constants, and a Haskell QuickCheck model with
  five trace properties plus a loopback-only live REST adapter. Dhall is
  verified on neo; the Haskell compile and run are deferred to sting.
refs:
  - TIN-4655
  - TIN-5105
  - R-C229
  - R-C227
  - SWB-R09
  - SWB-R14
  - SWB-R16
  - SWB-R53
  - R-N11
  - R-N12
  - R-N13
---

R-C229 / R-C227 / R-N13. This branch is stacked on PR #7 at head
`3d2b6469eaf07ddbb9cdaeb3b6a466ce16e99ac2`. It is
`feat/tin-4655-formal-spec-20261004` on the fork.

## What landed on the branch

- **Dhall** (`spec/dhall/`):
  - `ApprovedRelease.dhall` and `approved-broker.dhall`. The rendering equals
    `docs/releases/approved-broker.json` after `jq -S`.
  - `RustfsBucketIam.dhall`, typing blahaj `config/rustfs-iam/owners.json`.
    The snapshot is pinned at blahaj `a54ff72480657fbecae8960946714081df2adb0e`.
    The check is a strict json-to-dhall read plus a round trip.
  - `Broker.dhall` holds the constants and the `Unruled` SWB-R16 union. It is
    rendered to `generated/broker-constants.json`, which is freshness-checked.
- **Wiring:**
  - `spec/check-dhall.sh` does the checking.
  - The flake has checks `spec-dhall` and `spec-quickcheck`, package
    `swb-spec` (no IFD: a hand-written mkDerivation), app `spec-live`, and
    devShells `spec-dhall` and `spec`.
  - The just recipes are `spec-dhall`, `spec-owners`, `spec-quickcheck`,
    `spec-check`, `spec-live` and `remote-spec-check`.
- **Haskell** (`spec/haskell/`): a pure model plus five trace properties:
  - `leaseLifecycle`
  - `threadSeqDense`
  - `atLeastOnceUntilAck`
  - `exclusiveRefusal`
  - `noOrphanAfterExpiry`

  The properties judge results only, so `Swb.Live` reuses them against
  `/v1/*` on a loopback broker. Unruled SWB-R16 cases are never asserted.
- No file under the SWB-R53 protected roots changed (`crates/`, `deploy/`,
  the locks, and the rest).

## Verified on neo (interpreters and evaluation only)

- `just spec-dhall` passed: `spec-dhall: ok`. dhall 1.42.3 and dhall-json
  1.7.12 were substituted from cache.nixos.org at nixpkgs `c508844d`.
- The strict owners typing rejects an unknown field and a mistyped
  `cutover_order`. A drifted `manifest_size` in approved-broker.json fails
  the check with exit 1.
- `nix flake show` evaluates all outputs. `nix eval` of the x86_64-linux
  `checks.spec-quickcheck.drvPath` and `checks.spec-dhall.drvPath` succeeds.
- Locked haskellPackages: GHC 9.10.3, QuickCheck 2.15.0.1, aeson 2.2.4.1,
  http-client 0.7.19.

## Deferred to sting (not run; neo compiles nothing)

- `just remote-spec-check sting`, or on sting `just spec-check` or
  `nix build -L .#checks.x86_64-linux.spec-quickcheck`. **The Haskell code
  has never been compiled.** Expect possible small type fixes on the first
  build.
- The live run against a timeout-bounded loopback broker. The procedure is
  in `spec/README.md`.

## Guard refusal (R-N12)

One read-only `grep` that scanned the new files for process-control words was
refused by `tinyland-agent-process-claude-hook`, with the R-N11 message. I
stopped that line of work and did not reformulate the command.

## Open

- The clock seam and `spec/driver` follow-up. The ticket text is in
  `spec/README.md`. It touches `crates/`, so it needs a release or a ruling.
- Whether spec checks join `ci-ok`. No workflow was added, to avoid colliding
  with the secrets-scan and release edits to `ci.yml`.
- P2 questions: the three Unruled cases, the ended-holder orphan rule, claim
  lease expiry, and whether a claim refreshes the claimant's lease.

*Correction 2026-10-19 (TIN-6690):* R-C950 (TIN-5770 comment `23fdf965`)
ruled the three cases and the ended-holder rule (ending a session releases
its claims). `Broker.dhall` now lists `claim.rules` instead of `Unruled`, the
model and properties assert them, and Rust implements claims, leases and the
lease refresh.
