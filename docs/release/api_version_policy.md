# API Stability and Version Policy

## v3.1 Compatibility Contract
- Wire compatibility is strict for all existing fields and RPCs.
- Existing field numbers, oneofs, and enum numeric values are immutable.
- Existing RPC signatures are immutable:
  - `AppendEvent`
  - `GetManifest`
  - `Execute`
  - `Forget`

## Changes Allowed in v3.1
- Internal implementation changes with identical wire outputs for equivalent inputs.
- Additive documentation and test vectors.
- Bug fixes that preserve wire compatibility and deterministic outputs.

## Changes Requiring v3.2
- Any breaking proto change:
  - field type/number change
  - oneof restructuring
  - enum value renumbering
  - RPC request/response shape change
- Any semantic change that alters determinism contract (`semantic_root` or CPE response bytes) for existing vectors.

## SDK Proto Pinning
- Both SDKs pin proto contract using:
  - `proto_version = cortex_rmvm_v3_1`
  - source proto SHA-256 checksums
- Files:
  - `sdk/typescript/proto/proto_checksums.json`
  - `sdk/python/cortex_rmvm_sdk/generated/proto_checksums.json`

## CI Enforcement
- `scripts/check_proto_checksums.ps1` must pass.
- If proto files change, checksum manifests must update in the same PR.
- If checksum manifests change without proto file changes, fail CI.

## Release Rules
- Any intentional baseline drift requires:
  - dedicated PR label `determinism-baseline-update`
  - rationale for drift
  - linked conformance report diff artifacts
