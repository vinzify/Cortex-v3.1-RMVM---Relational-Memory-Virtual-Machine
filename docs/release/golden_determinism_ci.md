# Golden Determinism CI

## Matrix
- Linux: `ubuntu-latest`
- macOS: `macos-latest`
- Windows: `windows-latest`

## Per-OS Steps
- Run full workspace tests (`cargo test --workspace --locked`).
- Run conformance runner in check mode:
  - `cargo run --locked -p rmvm-tests --bin conformance_runner -- check`
- Export per-OS summary from conformance reports:
  - vector id
  - status
  - semantic root
  - CPE response SHA-256

## Cross-OS Drift Check
- Download summaries for all OS jobs.
- For each vector id:
  - require `status` equality
  - require `semantic_root` equality
  - require `response_sha256` equality
- Any mismatch fails CI and prints exact vector-level drift fields.

## Failure Artifacts
- Upload per-vector report JSONs on every run.
- On drift:
  - include OS-specific summary files
  - include expected vs actual hash values in compare output

## Baseline Update Policy
- Baseline files live in:
  - `tests/conformance/v1/baselines/<vector_id>/`
- Baseline changes require dedicated determinism update PR and rationale.
