---
title: "P1b OCI ABI correction and Honey runtime receipt"
date: 2026-09-27
status: active
summary: >-
  Pinned Debian 13 makes the Honey-built broker runnable, with Linux/amd64 packaging enforced by Bazel.
refs:
  - TIN-4655
---

## Mutation receipt (SWB-R04, R-N12, R-N13)

The operator approved the materially different R-N12 approach: remove the
unpublished public digest from the staged note, correct the ABI and platform,
then retry the normal signed commit. The base now uses the pinned Debian 13
nonroot manifest. The OCI layer, image and push targets require Linux/x86_64.
The three lock files were refreshed together on Honey; only the already staged
module lock changed. No registry push, deployment or process signalling occurred.

## Verification receipt (SWB-R04, R-N13)

- Honey `just image` built `//deploy:image` and `//deploy:image.digest`. The
  generated digest is the source for the PR check output, without a copy here.
- Honey `just check` passed all eight Bazel targets.
- Rootless Podman loaded the exact Bazel OCI layout. With networking disabled
  and the filesystem read-only, `swb version` exited 0 and reported version
  0.1.0 with envelope v3. Podman reported linux/amd64, UID 65532, entrypoint
  `/usr/local/bin/swb` and default command `serve`.
- Explicit Bazel image analysis for Linux/aarch64 failed on the x86_64 CPU
  constraint; macOS/x86_64 failed on the Linux OS constraint.

The image remains an unpublished candidate for a later blahaj rollout.
