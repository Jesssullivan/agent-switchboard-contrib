# Switchboard source and productionization lane

Authority: SWB-R49–SWB-R54, LAB-TAKEOVER-20260930 and R-N13.
Source/image ownership is here; lab delivers harness/Home Manager integration;
blahaj owns custody, retained storage, routing and cluster rollout (SWB-R51).

The approved production candidate is signed source
`b5158729355e836a3a98ade90d7649690e7a6900`. Its immutable image, original
PR inputs and publication readback are in
[`approved-broker.json`](../releases/approved-broker.json). That is the exact
SWB-R53 release; this source successor adds tooling and documentation without
rebuilding or approving another image.

## Source integration and upstream landing

1. Use `just local-integrate-named NAME 'PR@FULL_SHA ...'` for reviewed PR
   heads. It fetches current canonical main and exact heads, verifies source
   signatures, and creates signed merges in a new write-once worktree. It
   refuses reused paths/branches. Existing candidate trees remain intact.
   `local-integrate-named-dry-run` checks inputs without creating the tree.
2. `just release-check` checks the published source signature and ancestors,
   all 27 build/runtime input hashes, and absence of additional protected
   inputs. Supplied `--oci-layout PATH` checks the exact manifest plus every
   layer/config blob and linux/amd64, UID 65532, entrypoint and command.
   `--registry-manifest PATH` checks raw bytes from a recorded registry GET.
   This is read-only evidence validation; it makes no registry request and
   cannot admit a deployment or authorize a successor publication.
3. Source changes affecting protected inputs require a new reviewed build
   and bounded remote qualification on Honey, with Sting fallback. Bazel is
   the authority; never compile on Neo. The immutable candidate already has
   Honey `just check`, `build`, `image` and rootless startup receipts. Retain
   those exact-source receipts when only non-runtime tooling changes.
4. Upstream #2–#8 and #10 have merged; open PR heads are in GitHub, and the
   heads the SWB-R53 release was built from stay pinned in the release
   ledger. R49 authorizes local integration while GF is in development, but
   local integration does not satisfy the protected-main gate (signed
   commits, a fork PR and `ci-ok`, landed by the queue or by an R-C228 admin
   merge after sting validation). Do not enqueue or dispatch GF as part of
   this lane.

## Secrets scan and CODEOWNERS

- `ci-ok` now also requires the `secrets-scan` job: the pinned
  `xoxd-ai/ci-templates` secrets-scan action (TruffleHog `--only-verified`,
  then gitleaks 8.30.1 with `.gitleaks.toml`) over the full history. Like the
  Rust lane it is skipped on a fork PR and runs on `merge_group` and
  `push: main`. Run `nix develop --command just secrets-scan` on Sting (or any
  seat; it builds nothing) before a fork PR lands.
- `.github/CODEOWNERS` names the owner for every path and the release
  surfaces. The ruleset does not require code-owner review.
- The tag-triggered release workflow is not part of this change. Under
  R-C238 it lands in its own pull request before `v0.1.0`; until then nothing
  in this repository publishes an image.

## Packaging, retention and shutdown boundaries

The image is the Debian 13 distroless nonroot Linux/amd64 package declared in
`MODULE.bazel` and `deploy/BUILD.bazel`. The runtime base and image are digest
bound. Rootless `swb version` proved executable startup, not broker readiness
or an authenticated node pull. SWB-R53 covers this one image only.

SQLite uses WAL and FULL synchronization. `Store::open` prunes on startup;
the broker repeats pruning hourly without requiring mailbox traffic. Acked
bodies retain seven days, unacked bodies thirty days, and thread high-water
sequence survives pruning (SWB-R09). These are broker retention limits,
separate from state-store/backup retention commissioning.

The broker receives its own shutdown request, drains ordinary listeners for
thirty seconds, cancels remaining MCP streams, and allows five more seconds.
The tests use owned futures and channels, with no process signal. This bounds
listener futures; blocked SQLite work can still prevent full process exit.
The sixty-second pod grace does not prove storage failure recovery. Agent
process signaling remains forbidden (R-N11).

The existing service-proof source at `24c03ab9` passed 24 owned broker/store/
CLI cases on Sting, and lab's managed hooks passed 18 cases at exit zero and
under two seconds. That is useful development evidence. The tested CLI's
GLIBC_2.39 environment is separate from the approved packaged image. Do not
silently substitute that binary or its source for SWB-R53, or call owned
fixtures live harness enrollment or PVC pod-restart proof.

## Actual rollout prerequisites and exit receipts

The reviewed cluster source is blahaj draft
[#1731](https://github.com/xoxd-ai/blahaj/pull/1731) at
`ffbaf3ac982524887e58b3a5896bc5251654a3a2`. It composes the exact R53
image overlay with the custody inspector. TIN-5105 remains the custody owner
lane. Its current CREATEONLY/suspended qualifier and exclusive commissioning
window are unresolved; no broker hostname or live deployment is issued.

| Gate | Concrete next action and required evidence |
| --- | --- |
| Dedicated state custody | Blahaj owner completes the existing CREATEONLY decision, separate identity and encrypted carrier, primary history/deletion retention, off-node backup and isolated scratch recovery destination. Prove state payload, lock contention, unlock and scratch restore before reviewed backend release. Keep issuer credentials and Jobs in that owner's lane. |
| Namespace and image pull | After custody admission, use the reviewed governed namespace/Secret path; verify the exact digest with the target node and retain UID/RV and authenticated pull receipts. Client-side historical GET alone is insufficient. |
| Storage and route | Prove Bumble's retained 1 Gi RWO PVC is writable by UID/GID 65532, backup bootstrap and restore. Read actual Tailscale-issued MagicDNS from the live Service, then set the exact Host allowlist and lab client URL. Alias waits for route proof. |
| Harness delivery | Lab delivers the exact reviewed closure and verifies native Neo Claude/Sting Codex identities and Pi registration. Preserve combined Junie/Pi broker+LGTM paths and measured budgets. Enroll Honey/Bumble next, then Yoga/mbp-13; PZM remains storage held. |
| P1b functional acceptance | Record a threaded Neo Claude↔Sting Codex exchange and Pi reply/ack; durable inbox after an owner-admitted pod restart; broker-down nonblocking hook behavior plus #1920/Linear fallback; and exported registry URL matching the actually issued MagicDNS. Every receipt binds source, execution host and real session/message IDs. |

Usable L0 reads remain required for the combined paths. Telemetry scrubbing
and source-provenance qualification follow v1 and do not block MVP (SWB-R48);
message bodies remain disabled until their separate live ACL/redaction gates.
New backend applies, owner credentials, Jobs, restarts or down-fault tests
require the existing owner admission; this source packet does not grant it.
