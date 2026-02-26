# Hello Memory (TypeScript)

## 1. Start Server

```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

Server defaults:
- address: `127.0.0.1:50051`
- decode/encode message limits: `4 MiB`
- request timeout: `30s`

## 2. Run Hello Memory

```bash
cd sdk/typescript
npm ci
npm run build
npm run example:e2e
```

Expected output includes:
- plan-only prompt text
- verified blocks from `executePlan`
- forget confirmation block

## 3. Run STALL + Retry

```bash
npm run example:stall-retry
```

Expected output:
- first execution `STALL`
- second execution after retry `OK`
