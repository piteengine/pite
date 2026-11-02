# Install Pite (engine + player binaries for all platforms) from a release bundle, verified.
# Usage: .\install.ps1 [-Tag v0.1.0] [-Prefix "$env:LOCALAPPDATA\Pite"]
param(
  [string]$Tag = "",
  [string]$Prefix = (Join-Path $env:LOCALAPPDATA "Pite")
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repo = "piteengine/pite"
$headers = @{ "User-Agent" = "pite-installer" }
if (-not $Tag) {
  try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers $headers -UseBasicParsing
    $Tag = $release.tag_name
  } catch {
    if ($_.Exception.Message -like "*rate limit*") {
      throw "GitHub API rate limit reached. Specify the release version manually: .\install.ps1 -Tag vX.Y.Z"
    }
    throw "cannot resolve the latest release from GitHub: $_"
  }
}
if (-not $Tag) { throw "cannot resolve the latest release" }

$bundle = "pite-$Tag-windows.zip"
$base = "https://github.com/$repo/releases/download/$Tag"
$work = Join-Path ([IO.Path]::GetTempPath()) ("pite-install-" + [IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $work | Out-Null
try {
  Write-Host "Downloading Pite $Tag for Windows..."
  Invoke-WebRequest "$base/$bundle" -OutFile (Join-Path $work $bundle) -Headers $headers -UseBasicParsing
  Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $work "SHA256SUMS") -Headers $headers -UseBasicParsing
  $expected = (Select-String -Path (Join-Path $work "SHA256SUMS") -Pattern ([regex]::Escape($bundle)) |
    ForEach-Object { $_.Line.Split(" ")[0] })
  if (-not $expected) { throw "could not find checksum for $bundle in SHA256SUMS" }
  $actual = (Get-FileHash (Join-Path $work $bundle) -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne $expected.ToLower()) {
    throw "checksum mismatch for $bundle: expected $expected, got $actual"
  }

  New-Item -ItemType Directory -Path $Prefix -Force | Out-Null
  try {
    Expand-Archive (Join-Path $work $bundle) -DestinationPath $Prefix -Force
  } catch {
    throw "cannot extract to $Prefix (is pite.exe or pite-player running?): $_"
  }

  $path = [Environment]::GetEnvironmentVariable("PATH", "User")
  if ($path -notlike "*$Prefix*") {
    $newPath = if ([string]::IsNullOrEmpty($path)) { $Prefix } else { "$path;$Prefix" }
    [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    Write-Host "added $Prefix to the user PATH (restart the terminal to use it)"
  }
  if ($env:PATH -notlike "*$Prefix*") {
    $env:PATH = "$Prefix;$env:PATH"
  }
  Write-Host "installed $Tag to $Prefix (pite.exe, pite-player-windows.exe, pite-player-linux, pite-player-macos)"
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
