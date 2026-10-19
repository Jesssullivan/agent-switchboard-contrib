# Recorded demo

The recording of [examples/demo/run.sh](../../examples/demo/run.sh): every
mode `swb` has on `main` (v0.3.0, unreleased), driven by scripted fake
agents with no model calls and no secrets (R-C953, TIN-6694).

This is a scripted, one-machine precursor: one machine plays both hosts and
claims are stubbed. It does not satisfy TIN-6749 (recorded live demo: two
harnesses, two hosts) or TIN-6750 (full-features demo with claims and
handoff); both remain open.

| File | What |
| --- | --- |
| [transcript.txt](transcript.txt) | The normalized text transcript; `run.sh --check` diffs a fresh run against it |
| [demo.cast](demo.cast) | The same transcript as an asciicast v2 file, with fixed synthetic timing |

Play the cast with `asciinema play docs/demo/demo.cast`, or with any
asciinema-player embed. Regenerate both with
`SWB_BIN=/path/to/swb examples/demo/run.sh --record`.

The cast is a preview artifact: nothing here is published as a launch, and
claims, release and handoff appear only as "pending P2" stubs.
