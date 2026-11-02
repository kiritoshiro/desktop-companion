# Builds the Rust app and packs it as a portable folder and zip; the release
# workflow (.github/workflows/release-windows.yml) publishes the zip.
#
#   pwsh rust\package.ps1            # build, assemble, zip
#   pwsh rust\package.ps1 -NoBuild   # reuse the last release build
#
# Output: rust\dist\DesktopBugCompanion\ (DesktopBugCompanion.exe with
# models, personalities, presets and assets beside it, where the app looks
# for its data) and rust\dist\DesktopBugCompanion-<version>-windows-x64.zip.
param([switch]$NoBuild)
$ErrorActionPreference = "Stop"

$rust = $PSScriptRoot
$checkout = Split-Path $rust -Parent
$target = Join-Path $rust "target-release"
$dist = Join-Path $rust "dist"
$app = Join-Path $dist "DesktopBugCompanion"

$version = (Select-String -Path (Join-Path $rust "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

# Fail fast, before the build, if the packaged overlay is running. Deleting
# the exe first (below) is what actually protects the folder.
$running = Get-Process -Name DesktopBugCompanion -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($app + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) }
if ($running) { throw "Close the overlay first: it is running from $app (PID $($running.Id -join ', '))" }

if (-not $NoBuild) {
    $env:CARGO_TARGET_DIR = $target
    cargo build --release -p dc-app --manifest-path (Join-Path $rust "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$exe = Join-Path $target "release\dc-app.exe"
if (-not (Test-Path $exe)) { throw "No release build at $exe" }

if (Test-Path $app) {
    # A running overlay locks its exe, and a recursive delete would remove the
    # data folders before failing on it. Delete the exe first, so nothing else
    # goes while the overlay runs from here.
    $appExe = Join-Path $app "DesktopBugCompanion.exe"
    if (Test-Path $appExe) {
        try { Remove-Item -Force $appExe } catch { throw "Close the overlay first: $appExe is in use" }
    }
    Remove-Item -Recurse -Force $app
}
New-Item -ItemType Directory -Force $app | Out-Null
Copy-Item $exe (Join-Path $app "DesktopBugCompanion.exe")
foreach ($folder in "models", "personalities", "presets", "assets") {
    Copy-Item -Recurse (Join-Path $checkout $folder) (Join-Path $app $folder)
}
# Python bytecode or editor leftovers never ship.
Get-ChildItem $app -Recurse -Include "__pycache__", "*.pyc" -Force | Remove-Item -Recurse -Force

$zip = Join-Path $dist "DesktopBugCompanion-$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $app -DestinationPath $zip
$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
"Packaged $app"
"Zip: $zip ($size MB)"
