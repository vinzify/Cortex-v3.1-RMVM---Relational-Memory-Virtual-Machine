param(
  [string]$ApiKey = "demo-key",
  [string]$Brain = "",
  [string]$PlannerApiKey = ""
)

$env:CORTEX_ENDPOINT = "grpc://127.0.0.1:50051"
$env:CORTEX_BRAIN_SECRET = "demo-secret"
$env:CORTEX_PLANNER_MODE = "openai"
if ($PlannerApiKey) {
  $env:CORTEX_PLANNER_API_KEY = $PlannerApiKey
}
if (-not $env:CORTEX_PLANNER_API_KEY -and -not $env:OPENAI_API_KEY) {
  throw "Set -PlannerApiKey or OPENAI_API_KEY/CORTEX_PLANNER_API_KEY before running demo_m1."
}
$env:CORTEX_PLANNER_MODEL = if ($env:CORTEX_PLANNER_MODEL) { $env:CORTEX_PLANNER_MODEL } else { "gpt-4o-mini" }

if (-not $Brain) {
  $line = cargo run -p cortex-app -- brain list | Select-String "\[" | Select-Object -First 1
  if (-not $line) { throw "no brain found; run demo_m0 first" }
  $Brain = ($line -split '\[')[1].TrimEnd(']')
}

cargo run -p cortex-app -- auth map-key --api-key $ApiKey --tenant local --brain $Brain --subject user:local

Write-Host "Start rmvm-grpc server in terminal #1:"
Write-Host "  cargo run -p rmvm-grpc --bin rmvm-grpc-server"
Write-Host "Start proxy in terminal #2:"
Write-Host "  cd portable-brain-proxy"
Write-Host "  cargo run -p cortex-app -- proxy serve --addr 127.0.0.1:8080 --endpoint grpc://127.0.0.1:50051 --planner-mode openai"
Write-Host ""
Write-Host "OpenAI-compatible request (no RMVM/proto fields):"
Write-Host @"
curl -X POST http://127.0.0.1:8080/v1/chat/completions ^
  -H "Authorization: Bearer $ApiKey" ^
  -H "Content-Type: application/json" ^
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"I prefer oolong tea."}]}'
"@
