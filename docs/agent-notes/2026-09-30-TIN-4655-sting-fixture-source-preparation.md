# TIN-4655: exact owned fixture source prepared for Sting

Authority: LAB-TAKEOVER-20260930, SWB-R49, R-N13. No heavy execution,
compiler launch, live harness, cluster operation or process signal occurred.
Root retains capacity admission and serial slot ownership.

Executable candidate is signed 5ce2ab45ffdabb088f48b6c7e96c802839f070a8.
It requires nine Rust cases across the existing swb, swb-store and swb-broker
test targets, including the independently reviewed actual TCP/production
server/SQLite reopen fixture. Five driver mock tests pass. Actual Rust
qualification remains pending; native registration and live rollout remain
separate gates.

The exact Git bundle is 245351 bytes, SHA256
65d58ba845b9c1c9c77ec796cee7d738d10f9bf0e8769cafb04274bd30986fcc.
Neo durable copy is
`~/.claude/agent-notes-rescue/2026-09-30/swb-e2e-5ce2ab4.bundle`.
Sting copy and clean detached candidate checkout are respectively:

- `/srv/fast-local/jess/tmp/swb-e2e-5ce2ab4.bundle`
- `/srv/fast-local/jess/tmp/swb-e2e-source-5ce2ab4-20260930`

The received archive digest, full HEAD/signature and tracked/untracked clean
status were checked before declaring preparation complete. Repository source
only was transported; no credentials or private host state were bundled.
This documentation followup does not alter that prepared executable candidate.

Source pins Bazel9.2.0, rules_rust0.73.0 and Rust1.97.1, with146 crates in
cargo-bazel-lock.json. Sting PATH rustc1.95.0 is not the Bazel toolchain.
Read-only existing cache metadata established the matching Bazel installbase
`3e6f3b7d6fdac67aed908160850e082b` and Rust toolchain BUILD.version1.97.1.
The resolved user-owned Rust repository input cache occupies838620KiB;
Bazelisk downloads251484KiB. An initial du on the external symlink reported
zero, which was not interpreted as emptiness: its resolved repository-content
path was measured instead. Existing disk action cache221104KiB is not
configured by this driver, so no compiled action-cache hit is promised.
A private outputbase will still require compilation. Several GiB of output
is an estimate, not an observed result or a proven peak; duration is unknown.

Read-only filesystem headroom showed283893092352 free bytes on fast-local
and86019051520 free bytes on the home/cache filesystem. CPU capacity was not
reprobed during preparation. The last readiness-driven Honey sample at
07:48:33Z was load66.50/59.35/52.61, busy92.4%, available105651016kB,
CPU PSI some58.34%/full0: no heavy launch followed that failed envelope.
Root's current serial preference is native11 fixtures, then SWB owned broker
proof, then final CLI/image work after fresh capacity and slot reservation.

The proposed execution keeps one inherited allowed CPU, per-process6GiB AS
(not aggregate), JVM512MiB, bounded loading/globbing/test/action concurrency,
fixed arena2 for JVM and compiler/test environments, private batch outputbase
and the shared host heavy lock. No deadline or signal controls a process;
owned socket deadlines are local I/O bounds. Root must supply the Sting lock
path and reserve the slot before execution; the prepared checkout alone is
not admission.
