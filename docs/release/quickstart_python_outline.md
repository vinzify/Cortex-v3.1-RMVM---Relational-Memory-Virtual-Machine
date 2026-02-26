# Python Quickstart (Outline)

## 1. Prerequisites
- Python pinned version
- gRPC server running (`cargo run -p rmvm-grpc --bin server`)

## 2. Install
- `cd sdk/python`
- `python -m pip install -e .`

## 3. Initialize Client
- import `CortexRmvmClient`
- connect to `127.0.0.1:50051`

## 4. Append Event
- call `append_event(request_id, subject, text, scope)`
- capture returned handle refs

## 5. Get Manifest
- call `get_manifest(request_id)`
- inspect available handle/selector refs

## 6. Plan-Only Prompt
- call `build_plan_only_prompt(user_message, manifest)`
- enforce planner output is RMVMPlan JSON only

## 7. Validate + Execute
- call `validate_plan_against_manifest(plan, manifest)`
- call `execute_plan(request_id, manifest, plan)`

## 8. Display Verified Output
- print `verified_blocks`
- log `semantic_root` and `trace_root`

## 9. Forget Flow
- call `forget(request_id, subject, predicate_label, scope, reason)`
- display verified confirmation block

## 10. Troubleshooting
- map gRPC exceptions to status/error code outputs
- verify checksum manifest and conformance reports in CI
