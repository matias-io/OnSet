# Build the OnSet desktop installer: preparation screens, live view and the Python service
# with its own runtime, as a per-user NSIS setup.
#
# Needs: Node, Rust, the Tauri CLI (cargo install tauri-cli --version "^2" --locked), and the
# service environment from service/scripts/bootstrap.ps1. The installer bundles the service
# sources, the WSL worker files, the model manifest, rekordbox offsets, a portable Python
# runtime and the full-screen visuals (onset.exe, started from the OnSet Live window); music,
# workspaces, credentials and model caches are never included.
[CmdletBinding()]
param([switch]$SkipRuntime)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$node = 'C:/Program Files/nodejs/node.exe'
$npm = 'C:/Program Files/nodejs/node_modules/npm/bin/npm-cli.js'
Push-Location $root
try {
    Push-Location ui
    try {
        & $node $npm run build
        if ($LASTEXITCODE) { throw 'Interface build failed' }
    } finally { Pop-Location }

    if (-not $SkipRuntime) {
        & powershell -NoProfile -ExecutionPolicy Bypass -File service/scripts/package-runtime.ps1
        if ($LASTEXITCODE) { throw 'Packaged runtime preparation failed' }
    }
    if (-not (Test-Path -LiteralPath service/.artifacts/runtime/python.exe)) {
        throw 'service/.artifacts/runtime is missing; run without -SkipRuntime'
    }

    # The visuals ship beside onset-desktop.exe; stage the release build where the
    # installer configuration picks it up, wherever CARGO_TARGET_DIR points.
    cargo build --release -p onset-visuals
    if ($LASTEXITCODE) { throw 'Visuals build failed' }
    $built = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    New-Item -ItemType Directory -Force service/.artifacts/visuals | Out-Null
    Copy-Item -Force (Join-Path $built 'release/onset.exe') service/.artifacts/visuals/onset.exe

    Push-Location crates/onset-desktop
    try {
        cargo tauri build --config installer.conf.json
        if ($LASTEXITCODE) { throw 'Installer build failed' }
    } finally { Pop-Location }

    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    $setup = Get-ChildItem -LiteralPath (Join-Path $targetDir 'release/bundle/nsis') -Filter '*-setup.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw 'No installer was produced' }
    $hash = (Get-FileHash -LiteralPath $setup.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Host "Installer: $($setup.FullName)"
    Write-Host "Size: $($setup.Length) bytes"
    Write-Host "SHA-256: $hash"
} finally {
    Pop-Location
}
