## What and why

<!-- What this changes and the problem it solves. Link the issue. -->

## Rulings

<!-- Any ADR ruling (SWB-R##) this follows or changes. "None" is fine. -->

## Validation

<!-- What you ran and where. Fork PRs skip CI until the merge queue, so this matters. -->

- [ ] `just check`
- [ ] `just spec-dhall` (if `spec/` or `docs/releases/` changed)
- [ ] `just secrets-scan`
- [ ] Other:

## Checklist

- [ ] Every commit is signed off (`git commit -s`, DCO) and signed
- [ ] Docs updated (README, `docs/REFERENCE.md`) where behaviour changed
- [ ] `CHANGELOG.md` `[Unreleased]` entry added for user-visible changes
- [ ] No secrets, tokens or credential-bearing message bodies in code, tests or logs
