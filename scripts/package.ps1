# Build a portable OnSet folder from an explicit set of public files.
[CmdletBinding()]
param(
    [string]$BuildTargetDir = 'target',
    [string]$OutputRoot = 'dist',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipBuild) {
        cargo build --locked --release -p onset-visuals -p onset-cli --target-dir $BuildTargetDir
        if ($LASTEXITCODE -ne 0) { throw 'Build failed. A running executable may prevent replacement.' }
    }

    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $name = "OnSet-$version-windows"
    $outputBase = [System.IO.Path]::GetFullPath((Join-Path $root $OutputRoot))
    $out = Join-Path $outputBase $name
    $zip = Join-Path $outputBase "$name.zip"
    if ((Test-Path -LiteralPath $out) -or (Test-Path -LiteralPath $zip)) {
        throw "Output already exists. Choose a new -OutputRoot to preserve the previous package: $outputBase"
    }

    $executables = @('onset.exe', 'onset-cli.exe') | ForEach-Object { Join-Path $BuildTargetDir "release\$_" }
    foreach ($file in $executables + @('LICENSE', 'README.md', 'THIRD_PARTY_NOTICES.txt', 'assets\fonts\LICENSE')) {
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing package input: $file" }
    }
    foreach ($directory in @('offsets', 'assets\shaders', 'assets\fonts', 'docs\img')) {
        New-Item -ItemType Directory -Force (Join-Path $out $directory) | Out-Null
    }
    Copy-Item -LiteralPath $executables -Destination $out
    Copy-Item -LiteralPath LICENSE, README.md, THIRD_PARTY_NOTICES.txt -Destination $out
    Copy-Item -Path offsets\*.toml -Destination (Join-Path $out 'offsets')
    Copy-Item -Path assets\shaders\*.wgsl -Destination (Join-Path $out 'assets\shaders')
    Copy-Item -LiteralPath assets\fonts\LICENSE -Destination (Join-Path $out 'assets\fonts')
    Copy-Item -LiteralPath docs\img\onset.svg, docs\img\scenes.jpg, docs\img\lyrics.jpg -Destination (Join-Path $out 'docs\img')
    @"
OnSet $version - visuals for rekordbox

1. Start rekordbox 7 in Performance mode and load a track.
2. Double-click onset.exe.
3. Pick the screen and a style, then press Start show. Esc returns to the launcher.

For another rekordbox version, use Setup > Calibrate. The resulting file goes in
the offsets folder. Read README.md for supported versions, limitations and privacy.

Keys: H stats, C now-playing card, B blackout, Left/Right scene, F fullscreen,
Tab settings, Esc back to the launcher.

OnSet by Reimagine Media and Matias Suxo.
Free personal, noncommercial use under the OnSet Personal Use License 1.0.
Attribution and the same license are required for shared versions.
Commercial use and competing products require separate written permission.
Dependencies retain their own licenses.
See LICENSE, THIRD_PARTY_NOTICES.txt and assets/fonts/LICENSE.
"@ | Set-Content -Encoding utf8 (Join-Path $out 'QUICKSTART.txt')
    Compress-Archive -LiteralPath $out -DestinationPath $zip
    Write-Host "Packed $zip"
} finally {
    Pop-Location
}
