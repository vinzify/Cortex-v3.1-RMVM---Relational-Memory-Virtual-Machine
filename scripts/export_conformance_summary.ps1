param(
  [Parameter(Mandatory = $true)][string]$RunId,
  [Parameter(Mandatory = $true)][string]$OsId
)

$ErrorActionPreference = "Stop"

$repoRoot = Join-Path $PSScriptRoot ".."
$reportDir = Join-Path $repoRoot "tests\conformance\v1\reports\$RunId\$OsId"
if (!(Test-Path $reportDir)) {
  throw "Report directory does not exist: $reportDir"
}

$rows = @()
Get-ChildItem -Path $reportDir -Filter *.json | Sort-Object Name | ForEach-Object {
  $json = Get-Content $_.FullName -Raw | ConvertFrom-Json
  $rows += [PSCustomObject]@{
    vector_id = $json.vector_id
    status = $json.status
    success = [bool]$json.success
    response_sha256 = $json.response_sha256
    semantic_root = $json.semantic_root
  }
}

$summary = [PSCustomObject]@{
  run_id = $RunId
  os_id = $OsId
  vectors = $rows
}

$outPath = Join-Path $reportDir "summary.json"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $outPath -Encoding utf8
Write-Output "Wrote summary to $outPath"
