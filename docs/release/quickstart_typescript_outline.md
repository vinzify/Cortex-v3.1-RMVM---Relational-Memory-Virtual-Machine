# TypeScript Quickstart (Outline)

## 1. Prerequisites
- Node pinned version
- gRPC server running (`cargo run -p rmvm-grpc --bin server`)

## 2. Install
- `cd sdk/typescript`
- `npm ci`

## 3. Initialize Client
- import `CortexRmvmClient`
- connect to `127.0.0.1:50051`

## 4. Append Event
- call `appendEvent({ requestId, subject, text, scope })`
- capture returned handle refs

## 5. Get Manifest
- call `getManifest(requestId)`
- inspect allowed `handle` and `selector` refs

## 6. Plan-Only Prompt
- call `buildPlanOnlyPrompt(userMessage, manifest)`
- send prompt to planner/model constrained to plan JSON output

## 7. Validate + Execute
- call `validatePlanAgainstManifest(plan, manifest)`
- call `executePlan({ requestId, manifest, plan })`

## 8. Display Verified Output
- print `verifiedBlocks`
- log `semanticRoot` and `traceRoot`

## 9. Forget Flow
- call `forget({ requestId, subject, predicateLabel, scope, reason })`
- display verified confirmation block

## 10. Troubleshooting
- map gRPC errors to conformance error codes
- check conformance baseline drift before release
