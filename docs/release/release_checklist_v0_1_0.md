# Release Checklist v0.1.0 (Go/No-Go)

## Decision Rule
- GO only if every required checkbox below is checked.
- Any unchecked required item is NO-GO for tagging and release.

## Required Checks
- [ ] Conformance + determinism CI green on `main`
  - Workflow: `.github/workflows/determinism-ci.yml`
  - Required jobs:
    - `Conformance (linux|macos|windows)`
    - `Compare Cross-OS Determinism`
    - `SDK Wire Compatibility`

- [ ] Proto checksum gates green
  - `./scripts/check_proto_checksums.ps1` passes locally and in CI.

- [ ] Dependency hash gate green
  - `./scripts/check_dependency_manifest_hash.ps1` passes locally and in CI.

- [ ] Security audit v1 accepted (no open blocker findings)
  - Audit file: `docs/security/audit_v1.md`
  - Acceptance condition:
    - no open findings marked severity `blocker`
    - owners and remediation PRs assigned for non-blocker gaps

- [ ] License files present (root + SDKs)
  - `LICENSE`
  - `sdk/typescript/LICENSE`
  - `sdk/python/LICENSE`

- [ ] Changelog entry exists for `v0.1.0`
  - `CHANGELOG.md` includes dated `## [0.1.0]` release notes.

- [ ] `rmvm-grpc-server` binaries verified on all OSes with golden ExecuteRequest
  - Verification source:
    - CI `transport_golden` test on linux/macos/windows (deterministic ExecuteResponse bytes + proof roots)
    - release workflow binary build matrix green before publish

## Release Approval
- Release manager: __________________
- Date: __________________
- Final decision: `GO` / `NO-GO`
