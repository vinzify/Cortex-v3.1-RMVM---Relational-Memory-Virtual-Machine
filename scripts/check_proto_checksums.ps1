$ErrorActionPreference = "Stop"

$repoRoot = Join-Path $PSScriptRoot ".."
$coreProto = Join-Path $repoRoot "proto\cortex_rmvm_v3_1.proto"
$svcProto = Join-Path $repoRoot "proto\cortex_rmvm_v3_1_service.proto"

function Get-CanonicalTextSha256([string]$Path) {
  if (!(Test-Path $Path)) {
    throw "Missing proto file: $Path"
  }

  $raw = Get-Content $Path -Raw
  # Canonicalize line endings so checks are stable across checkout modes/OSes.
  $normalized = $raw.Replace("`r`n", "`n").Replace("`r", "`n")
  $bytes = ([System.Text.UTF8Encoding]::new($false)).GetBytes($normalized)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hashBytes = $sha.ComputeHash($bytes)
  }
  finally {
    $sha.Dispose()
  }
  return ([System.BitConverter]::ToString($hashBytes)).Replace("-", "").ToLowerInvariant()
}

$actual = @{
  "proto/cortex_rmvm_v3_1.proto" = Get-CanonicalTextSha256 $coreProto
  "proto/cortex_rmvm_v3_1_service.proto" = Get-CanonicalTextSha256 $svcProto
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
