# Portable Brain + Proxy UX

Standalone Rust workspace layered on top of Cortex v3.1 RMVM without changing proto/kernel contracts.

## Workspace modules
- `brain-store`: local encrypted brain workspaces (create/use/list/export/import/branch/merge/forget/attach/audit).
- `adapter-rmvm`: gRPC adapter for `AppendEvent`, `GetManifest`, `Execute`, `Forget`.
- `planner-guard`: plan-only prompt builder + RMVM plan validation.
- `cortex-app`: `cortex` CLI and OpenAI-compatible proxy endpoint.

## Environment UX
- `CORTEX_BRAIN`: default brain id/name for CLI and proxy fallback.
- `CORTEX_ENDPOINT`: RMVM gRPC endpoint (default `grpc://127.0.0.1:50051`).
- `CORTEX_BRAIN_SECRET`: required passphrase for brain encryption/decryption.
- `CORTEX_PLANNER_MODE`: `openai`, `byo`, or `fallback`.
- `CORTEX_PLANNER_BASE_URL`: planner API base URL (default `https://api.openai.com/v1`).
- `CORTEX_PLANNER_MODEL`: planner model name.
- `CORTEX_PLANNER_API_KEY`: planner API key (falls back to `OPENAI_API_KEY`).
- `OPENAI_BASE_URL`: set this in OpenAI-compatible clients to point at proxy (`http://127.0.0.1:8080/v1`).

## Build and test
```bash
cd portable-brain-proxy
cargo test
```

## CLI commands
```bash
cortex brain create <name> [--tenant <id>] [--passphrase-env <ENV>]
cortex brain use <brain-id-or-name>
cortex brain list [--json]
cortex brain export <brain-id-or-name> --out <file.cbrain>
cortex brain import --in <file.cbrain> [--name <alias>] [--verify-only]
cortex brain branch <brain-id-or-name> --new <branch-name>
cortex brain merge --source <branch> --target <branch> [--strategy ours|theirs|manual] [--brain <id>]
cortex brain forget --subject <subject> --predicate <predicate> [--scope <scope>] [--reason <text>] [--brain <id>]
cortex brain attach --agent <id> --model <id> --read <csv> --write <csv> --sinks <csv> [--ttl <duration>] [--brain <id>]
cortex proxy serve --addr 127.0.0.1:8080 --endpoint grpc://127.0.0.1:50051 --planner-mode openai
cortex auth map-key --api-key <key> --tenant <tenant> --brain <brain-id>
```

## Zero Integration Quickstart
1. Start RMVM gRPC:
```bash
cargo run -p rmvm-grpc --bin rmvm-grpc-server
```

2. Create and select a brain:
```bash
cd portable-brain-proxy
set CORTEX_BRAIN_SECRET=demo-secret   # Windows PowerShell: $env:CORTEX_BRAIN_SECRET="demo-secret"
cargo run -p cortex-app -- brain create demo
cargo run -p cortex-app -- brain use demo
cargo run -p cortex-app -- auth map-key --api-key demo-key --tenant local --brain demo --subject user:local
```

3. Start proxy with planner mode:
```bash
set CORTEX_PLANNER_MODE=openai
set CORTEX_PLANNER_API_KEY=<your-openai-compatible-key>
set CORTEX_PLANNER_MODEL=gpt-4o-mini
cargo run -p cortex-app -- proxy serve --addr 127.0.0.1:8080 --endpoint grpc://127.0.0.1:50051
```

4. Use any OpenAI-compatible client by changing only `OPENAI_BASE_URL`:
```bash
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer demo-key" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"I prefer oolong tea."}]}'
```

5. Optional deterministic planner testing (`BYO`):
```bash
set CORTEX_PLANNER_MODE=byo
curl -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer demo-key" \
  -H "X-Cortex-Plan: <base64-plan-json>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"I prefer tea."}]}'
```

## Demo scripts
- `scripts/demo_m0.ps1`
- `scripts/demo_m1.ps1`
- `scripts/demo_m2.ps1`
