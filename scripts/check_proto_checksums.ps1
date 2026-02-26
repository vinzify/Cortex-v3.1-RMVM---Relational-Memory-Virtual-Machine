$ErrorActionPreference = "Stop"

$repoRoot = Join-Path $PSScriptRoot ".."
$coreProto = Join-Path $repoRoot "proto\cortex_rmvm_v3_1.proto"
$svcProto = Join-Path $repoRoot "proto\cortex_rmvm_v3_1_service.proto"

$actual = @{
  "proto/cortex_rmvm_v3_1.proto" = (Get-FileHash $coreProto -Algorithm SHA256).Hash.ToLowerInvariant()
  "proto/cortex_rmvm_v3_1_service.proto" = (Get-FileHash $svcProto -Algorithm SHA256).Hash.ToLowerInvariant()
}

$manifests = @(
  (Join-Path $repoRoot "sdk\typescript\proto\proto_checksums.json"),
  (Join-Path $repoRoot "sdk\python\cortex_rmvm_sdk\generated\proto_checksums.json")
)

foreach ($manifestPath in $manifests) {
  if (!(Test-Path $manifestPath)) {
    throw "Missing proto checksum manifest: $manifestPath"
  }
  $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.proto_version -ne "cortex_rmvm_v3_1") {
    throw "Unexpected proto_version in ${manifestPath}: $($manifest.proto_version)"
  }
  foreach ($k in $actual.Keys) {
    $expected = $manifest.checksums.$k
    if ([string]::IsNullOrWhiteSpace($expected)) {
      throw "Missing checksum key '$k' in $manifestPath"
    }
    if ($expected.ToLowerInvariant() -ne $actual[$k]) {
      throw "Checksum mismatch for '$k' in $manifestPath expected=$expected actual=$($actual[$k])"
    }
  }
}

Write-Output "Proto checksum manifests match source protos."
