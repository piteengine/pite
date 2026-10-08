# Install Pite (engine + both players) from a release bundle, verified.
# Usage: .\install.ps1 [-Tag v0.1-m5] [-Prefix "$env:LOCALAPPDATA\Pite"]
param(
  [string]$Tag = "",
  [string]$Prefix = (Join-Path $env:LOCALAPPDATA "Pite")
)
$ErrorActionPreference = "Stop"

$repo = "piteengine/pite"
if (-not $Tag) {
  $Tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
if (-not $Tag) { throw "cannot resolve the latest release" }

$bundle = "pite-$Tag-windows.zip"
$base = "https://github.com/$repo/releases/download/$Tag"
$work = Join-Path ([IO.Path]::GetTempPath()) ("pite-install-" + [IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $work | Out-Null
try {
  Invoke-WebRequest "$base/$bundle" -OutFile (Join-Path $work $bundle)
  Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $work "SHA256SUMS")
  $expected = (Select-String -Path (Join-Path $work "SHA256SUMS") -Pattern ([regex]::Escape($bundle)) |
    ForEach-Object { $_.Line.Split(" ")[0] })
  $actual = (Get-FileHash (Join-Path $work $bundle) -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne $expected.ToLower()) { throw "checksum mismatch for $bundle" }

  New-Item -ItemType Directory -Path $Prefix -Force | Out-Null
  Expand-Archive (Join-Path $work $bundle) -DestinationPath $Prefix -Force

  $path = [Environment]::GetEnvironmentVariable("PATH", "User")
  if ($path -notlike "*$Prefix*") {
    [Environment]::SetEnvironmentVariable("PATH", "$path;$Prefix", "User")
    Write-Host "added $Prefix to the user PATH (restart the terminal to use it)"
  }
  Write-Host "installed $Tag to $Prefix (pite.exe, pite-player.exe, pite-player)"
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
