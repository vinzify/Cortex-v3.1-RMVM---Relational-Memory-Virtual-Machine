param(
  [string]$Brain = ""
)

$env:CORTEX_BRAIN_SECRET = "demo-secret"

if (-not $Brain) {
  $line = cargo run -p cortex-app -- brain list | Select-String "\[" | Select-Object -First 1
  if (-not $line) { throw "no brain found" }
  $Brain = ($line -split '\[')[1].TrimEnd(']')
}

cargo run -p cortex-app -- brain branch $Brain --new exp-a
cargo run -p cortex-app -- brain attach --brain $Brain --agent local-agent --model gpt-4o-mini --read normative.preference --write normative.preference --sinks none
cargo run -p cortex-app -- brain merge --brain $Brain --source exp-a --target main --strategy ours
cargo run -p cortex-app -- brain forget --brain $Brain --subject user:local --predicate prefers_beverage --scope SCOPE_GLOBAL --reason "suppress preference"
cargo run -p cortex-app -- brain audit --brain $Brain --json
