# Cortex v3.1 RMVM

Cortex v3.1 RMVM is a deterministic relational memory virtual machine with:
- strict protobuf interfaces,
- kernel-validated execution plans,
- deterministic semantic roots and proof artifacts,
- gRPC service endpoints and language SDKs.

## Packages
- Rust crates:
  - `rmvm-proto`
  - `rmvm-kernel`
- TypeScript SDK:
  - `@cortex/rmvm-sdk`
- Python SDK:
  - `cortex-rmvm-sdk`

## Local Validation

```bash
cargo test --workspace
cargo run -p rmvm-tests --bin conformance_runner -- check
```

## Quickstarts
- TypeScript: `docs/quickstarts/hello_memory_typescript.md`
- Python: `docs/quickstarts/hello_memory_python.md`

## Release
- Tag-based release pipeline: `.github/workflows/release.yml`
- Artifact publish/runbook: `docs/release/publish_artifacts.md`
