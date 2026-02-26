param(
  [string]$Secret = "demo-secret"
)

$env:CORTEX_BRAIN_SECRET = $Secret

Write-Host "Creating brain..."
cargo run -p cortex-app -- brain create demo --tenant local --passphrase-env CORTEX_BRAIN_SECRET

Write-Host "Listing brains..."
cargo run -p cortex-app -- brain list

Write-Host "Export/import roundtrip..."
$brainLine = cargo run -p cortex-app -- brain list | Select-String "\["
$brainId = ($brainLine -split '\[')[1].TrimEnd(']')
cargo run -p cortex-app -- brain export $brainId --out demo.cbrain
cargo run -p cortex-app -- brain import --in demo.cbrain --name demo-copy
cargo run -p cortex-app -- brain list
