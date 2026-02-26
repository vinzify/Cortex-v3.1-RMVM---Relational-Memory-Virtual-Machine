# Publish Artifacts

## Release Trigger
- Push a tag matching `v*` (example: `v0.1.0`).
- CI workflow: `.github/workflows/release.yml`.

## Required Repository Secrets
- `CARGO_REGISTRY_TOKEN`
- `NPM_TOKEN`
- `PYPI_API_TOKEN`

## Published Outputs
- crates.io:
  - `rmvm-proto`
  - `rmvm-kernel`
- npm:
  - `@cortex/rmvm-sdk`
- PyPI:
  - `cortex-rmvm-sdk`
- GitHub Release assets:
  - `rmvm-grpc-server-linux-<arch>`
  - `rmvm-grpc-server-macos-<arch>`
  - `rmvm-grpc-server-windows-<arch>.exe`
  - per-file SHA-256 sidecars (`.sha256`)

## Pre-Tag Local Verification
- `cargo test --workspace --locked`
- `cargo run --locked -p rmvm-tests --bin conformance_runner -- check`
- `./scripts/check_proto_checksums.ps1`
- `./scripts/check_dependency_manifest_hash.ps1`
- `cd sdk/typescript && npm ci --include=dev && npm run build && npm pack --dry-run`
- `cd sdk/python && py -m pip install --upgrade build twine && py -m build && py -m twine check dist/*`

## Publish Order Notes
- `rmvm-proto` publishes before `rmvm-kernel`.
- `rmvm-kernel` publish step retries to absorb crates.io index propagation delay.

## Post-Release Verification
- crates.io pages list `0.1.0` (or tag version) for both crates.
- npm shows latest `@cortex/rmvm-sdk`.
- PyPI shows latest `cortex-rmvm-sdk` and both wheel + sdist.
- GitHub Release includes three platform binaries and checksums.
