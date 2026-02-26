# Security Audit v1

## Run Metadata
- Date: 2026-02-26
- Scope: RMVM kernel, gRPC transport, conformance vectors, SDK packaging gates
- Auditor: Codex (local run in `D:\code\cortex`)

## Executed Checks
- `cargo test --workspace --locked` -> PASS
- `cargo run --locked -p rmvm-tests --bin conformance_runner -- check` -> PASS (`32/32` vectors)
- `cargo test -p rmvm-grpc --test transport_golden --locked` -> PASS
- `./scripts/check_proto_checksums.ps1` -> PASS
- `./scripts/check_dependency_manifest_hash.ps1` -> PASS
- `cd sdk/typescript && npm run build && npm pack --dry-run` -> PASS
- `cd sdk/python && py -m build && py -m twine check dist/*` -> PASS

## Checklist Results

### Metadata Sanitization (Trojan TOC)
- Status: FAIL (coverage gap)
- Evidence:
  - No automated tests currently validate bidi-control/homoglyph sanitization in metadata fields.
  - No explicit metadata max-byte boundary tests were found in conformance vectors or gRPC transport tests.
- Owner: Runtime Security (Kernel + Manifest ingestion)
- Remediation PR: `sec/metadata-sanitization-trojan-toc-tests`

### Narrative Token-Guard Fuzzing
- Status: PARTIAL
- Evidence:
  - Guard violations are enforced in conformance (`C31-NARR-001`, `C31-NARR-002`) and pass.
  - No fuzz corpus/coverage measurement exists for token boundaries/macros.
- Owner: Runtime Security (Narrative Guard)
- Remediation PR: `sec/narrative-token-guard-fuzz-corpus-v1`

### Capability Sink Enforcement
- Status: PASS (deterministic conformance coverage)
- Evidence:
  - Trust gate: `C31-SEC-005` pass.
  - Taint sink gate: `C31-SEC-006`, `C31-SEC-007` pass.
  - Broken lineage propagation: `C31-SEC-003`, `C31-SEC-004` pass.
- Owner: Kernel Execution
- Remediation PR: N/A

### gRPC Abuse Cases
- Status: FAIL (coverage gap)
- Evidence:
  - Oversized message limits are configurable in server runtime, but no automated transport-level oversized request/response tests found.
  - No automated malformed protobuf/replay-abuse/high-rate abuse test cases found.
- Owner: gRPC Transport
- Remediation PR: `sec/grpc-abuse-suite-v1`

### Release Blocking Conditions
- Status: FAIL
- Evidence:
  - Not all checklist groups are mapped to CI enforcement yet (metadata sanitization + gRPC abuse suite gaps).
  - Determinism/conformance artifacts are present and validated.
- Owner: Release Engineering
- Remediation PR: `sec/release-gate-security-checklist-enforcement`

## Findings Summary
- High:
  - Missing metadata sanitization adversarial test coverage.
  - Missing automated gRPC abuse suite (oversize/malformed/replay/rate abuse).
- Medium:
  - Narrative token-guard fuzzing corpus and coverage reporting not yet implemented.
- Low:
  - None.

## Immediate Next Actions
1. Implement `sec/metadata-sanitization-trojan-toc-tests` and add to CI required checks.
2. Implement `sec/grpc-abuse-suite-v1` transport tests and enforce in CI.
3. Implement `sec/narrative-token-guard-fuzz-corpus-v1` with reproducible corpus + coverage export.
4. Enforce checklist gate via `sec/release-gate-security-checklist-enforcement` before release tagging.
