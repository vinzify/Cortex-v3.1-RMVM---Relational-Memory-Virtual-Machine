# Conformance Suite v1

## Scope
- Repo-level vector-driven conformance checks for RMVM v3.1.
- Required assertions:
  - `ExecutionStatus` and `ErrorCode`
  - `semantic_root` determinism
  - CPE-encoded `ExecuteResponse` byte determinism
  - optional verified render block exact-match

## Vector Format
- Location: `tests/conformance/v1/vectors/<category>/<vector_id>.json`
- Required top-level fields:
  - `vector_id`
  - `spec_version` (`conformance/v1.0.0`)
  - `proto_version` (`cortex_rmvm_v3_1`)
  - `description`
  - `manifest`
  - `plan`
  - `expect`
  - `determinism`
- Optional:
  - `execute_options` (`allow_partial_on_stall`, `degraded_mode`, `broken_lineage_handles`, `narrative_templates`)

## Directory Layout
- `tests/conformance/v1/schema/vector.schema.json`
- `tests/conformance/v1/vectors/*/*.json`
- `tests/conformance/v1/baselines/<vector_id>/expected.execute_response.cpe.pb`
- `tests/conformance/v1/baselines/<vector_id>/expected.meta.json`
- `tests/conformance/v1/reports/<run_id>/<os>/<vector_id>.json`

## Naming Convention
- Vector id pattern: `C31-<CATEGORY>-<NNN>-<slug>`
- Category folders: `core`, `ref`, `type`, `ssa`, `cost`, `sec`, `stall`, `narr`, `det`

## Versioning Rules
- `conformance/vMAJOR.MINOR.PATCH`
- MAJOR:
  - incompatible vector schema changes
- MINOR:
  - additive vectors/categories
- PATCH:
  - expectation/baseline corrections only

## Baseline and Sync
- Sync command:
  - `cargo run -p rmvm-tests --bin conformance_runner -- sync`
- Check command:
  - `cargo run -p rmvm-tests --bin conformance_runner -- check`
- Sync updates:
  - vector expected status/error/stall/render roots
  - baseline CPE protobuf bytes
  - baseline metadata hashes
