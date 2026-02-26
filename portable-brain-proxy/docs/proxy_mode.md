# Proxy Mode v0

`cortex proxy serve` exposes an OpenAI-compatible endpoint.

## Endpoint
- `POST /v1/chat/completions`

## Internal flow
1. Authenticate `Authorization: Bearer <api-key>`.
2. Resolve API key to `tenant_id + brain_id` mapping.
3. Append user message via `AppendEvent`.
4. Fetch `PublicManifest` via `GetManifest`.
5. Build + enforce plan-only prompt constraints.
6. Generate `RMVMPlan` via planner mode (`openai`, `byo`, or `fallback`) and validate against manifest refs.
7. Execute via `Execute`.
8. Return verified blocks in OpenAI-compatible payload.

## Status mapping
- `OK` -> HTTP `200`
- `STALL` -> HTTP `503`, `code: cortex_stall`
- `REJECTED` -> HTTP `400`, `code: cortex_rejected_<ERROR_CODE>`

## Proof surfacing
- JSON: `cortex.semantic_root`, `cortex.trace_root`
- Headers: `X-Cortex-Semantic-Root`, `X-Cortex-Trace-Root`

## Planner modes
- `openai`: calls an OpenAI-compatible planner endpoint and requires `CORTEX_PLANNER_API_KEY` (or `OPENAI_API_KEY`).
- `byo`: requires `X-Cortex-Plan: <base64 RMVMPlan JSON>` header on each request.
- `fallback`: deterministic local plan generation for development fallback.

## Modes
- Local mode: `--endpoint grpc://127.0.0.1:50051`
- Cloud mode: same adapter API, different endpoint URL later

## Environment UX
- `CORTEX_BRAIN` default brain
- `CORTEX_ENDPOINT` RMVM endpoint
- `CORTEX_PLANNER_MODE` planner mode (`openai|byo|fallback`)
- `CORTEX_PLANNER_BASE_URL` planner base URL (default `https://api.openai.com/v1`)
- `CORTEX_PLANNER_MODEL` planner model name
- `CORTEX_PLANNER_API_KEY` planner key
- `OPENAI_BASE_URL` point existing clients to proxy `/v1`
