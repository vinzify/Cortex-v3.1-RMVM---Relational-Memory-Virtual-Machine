# RMVM gRPC Server Configuration

## Runtime Environment Variables
- `RMVM_SERVER_ADDR`
  - default: `127.0.0.1:50051`
- `RMVM_MAX_DECODING_BYTES`
  - default: `4194304` (4 MiB)
  - rejects larger inbound gRPC messages
- `RMVM_MAX_ENCODING_BYTES`
  - default: `4194304` (4 MiB)
  - rejects larger outbound gRPC messages
- `RMVM_REQUEST_TIMEOUT_SECS`
  - default: `30`
  - unary request timeout at server layer

## Determinism Requirements (Operational)
- All production builds must use pinned toolchain:
  - `rust-toolchain.toml` (`1.93.1`)
- Do not deploy with modified protobuf/codegen dependency versions unless:
  - conformance suite passes
  - cross-OS determinism check passes
- Required pre-release checks:
  - `cargo test --workspace --locked`
  - `cargo run -p rmvm-tests --bin conformance_runner -- check`
  - proto checksum check script
  - dependency manifest hash check script

## Baseline Update Policy
- Baselines live under:
  - `tests/conformance/v1/baselines/`
- Update process:
  1. run `cargo run -p rmvm-tests --bin conformance_runner -- sync`
  2. review baseline `expected.meta.json` diffs per vector
  3. require determinism rationale in PR description
  4. require cross-OS CI green before merge
- Any unexplained baseline drift is release-blocking.
