# Changelog

All notable changes to this project are documented in this file.

## [0.1.0] - 2026-02-26

### Added
- RMVM kernel MVP with deterministic execution, semantic hashing, proof roots, lineage handling, trust/taint gates, and guarded narrative behavior.
- gRPC service layer (`AppendEvent`, `GetManifest`, `Execute`, `Forget`) with deterministic transport behavior.
- Transport-level golden tests with byte-identical `ExecuteResponse` and proof root checks.
- Conformance suite v1 with 32 vectors including adversarial/security and determinism invariance coverage.
- Cross-OS determinism CI and drift comparison/report scripts.
- TypeScript SDK (`@cortex/rmvm-sdk`) with plan-only prompt helper, manifest-ref validation, and runnable `e2e` + `stall_retry` examples.
- Python SDK (`cortex-rmvm-sdk`) with matching surface and runnable `e2e` + `stall_retry` examples.
- Operational docs for server config, determinism requirements, baseline policy, and forget UX semantics.
- Security audit record `docs/security/audit_v1.md`.
- Release automation for crates/npm/PyPI + GitHub release binary/checksum assets.
