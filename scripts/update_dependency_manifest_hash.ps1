$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$checkScript = Join-Path $repoRoot "scripts\check_dependency_manifest_hash.ps1"
$baselinePath = Join-Path $repoRoot ".ci\dependency-manifest.sha256"

$files = @(
  "Cargo.toml",
  "Cargo.lock"
)
Get-ChildItem -Recurse -Filter Cargo.toml (Join-Path $repoRoot "crates") | ForEach-Object {
  $files += $_.FullName.Substring($repoRoot.Length + 1).Replace("\", "/")
}
$files = $files | Sort-Object

$builder = New-Object System.Text.StringBuilder
foreach ($rel in $files) {
  [void]$builder.Append("--- ").Append($rel).Append(" ---`n")
  $raw = Get-Content (Join-Path $repoRoot $rel) -Raw
  [void]$builder.Append($raw.Replace("`r`n", "`n")).Append("`n")
}
$bytes = ([System.Text.UTF8Encoding]::new($false)).GetBytes($builder.ToString())
$sha = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha.ComputeHash($bytes)
$hash = ([System.BitConverter]::ToString($hashBytes)).Replace("-", "").ToUpperInvariant()

[System.IO.File]::WriteAllText($baselinePath, $hash + "`n", [System.Text.UTF8Encoding]::new($false))
Write-Output "Updated $baselinePath to $hash"
