$ErrorActionPreference = "Stop"

param(
  [Parameter(Mandatory = $true)][string]$LinuxSummary,
  [Parameter(Mandatory = $true)][string]$MacSummary,
  [Parameter(Mandatory = $true)][string]$WindowsSummary
)

function Load-ById([string]$Path) {
  $json = Get-Content $Path -Raw | ConvertFrom-Json
  $map = @{}
  foreach ($row in $json.vectors) {
    $map[$row.vector_id] = $row
  }
  return $map
}

$linux = Load-ById $LinuxSummary
$mac = Load-ById $MacSummary
$win = Load-ById $WindowsSummary

$ids = $linux.Keys | Sort-Object
if ($ids.Count -eq 0) {
  throw "No vectors found in linux summary."
}

$failures = @()
foreach ($id in $ids) {
  if (!$mac.ContainsKey($id) -or !$win.ContainsKey($id)) {
    $failures += "$id missing from one or more summaries"
    continue
  }
  $l = $linux[$id]
  $m = $mac[$id]
  $w = $win[$id]

  if (!$l.success -or !$m.success -or !$w.success) {
    $failures += "$id has non-success result in one or more OS summaries"
  }
  if ($l.response_sha256 -ne $m.response_sha256 -or $l.response_sha256 -ne $w.response_sha256) {
    $failures += "$id response_sha256 drift linux=$($l.response_sha256) mac=$($m.response_sha256) win=$($w.response_sha256)"
  }
  if ($l.semantic_root -ne $m.semantic_root -or $l.semantic_root -ne $w.semantic_root) {
    $failures += "$id semantic_root drift linux=$($l.semantic_root) mac=$($m.semantic_root) win=$($w.semantic_root)"
  }
}

if ($failures.Count -gt 0) {
  Write-Error ("Determinism drift detected:`n- " + ($failures -join "`n- "))
  exit 1
}

Write-Output "Cross-platform conformance summaries are deterministic."
