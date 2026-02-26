# TypeScript SDK

## Prerequisites
- Node 24+
- RMVM gRPC server running locally

Start server:

```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

## Install + Build

```bash
cd sdk/typescript
npm ci
npm run build
```

## Hello Memory (Mocked Plan)

```bash
npm run example:e2e
```

What it does:
- appends a user preference event
- fetches manifest
- builds a plan-only prompt
- executes a mocked `RMVMPlan`
- prints verified blocks and forget confirmation

## STALL + Retry

```bash
npm run example:stall-retry
```

What it does:
- appends an event and fetches manifest
- executes with mocked `OFFLINE` availability to force `STALL`
- flips availability to `READY`
- retries the same plan and prints final status
