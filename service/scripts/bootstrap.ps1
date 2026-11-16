$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$uvCommand = Get-Command uv -ErrorAction SilentlyContinue
$uvPath = if ($uvCommand) { $uvCommand.Source } else { Join-Path $root '.runtime/Scripts/uv.exe' }
if (-not (Test-Path -LiteralPath $uvPath)) {
    throw 'Install uv from https://docs.astral.sh/uv/getting-started/installation/, then run this script again.'
}
& $uvPath python install 3.11 --install-dir (Join-Path $root '.managed-python')
if ($LASTEXITCODE) { throw 'Managed Python installation failed' }
$python = Get-ChildItem -LiteralPath (Join-Path $root '.managed-python') -Directory | Where-Object Name -Like 'cpython-3.11*' | Select-Object -First 1
& $uvPath sync --locked --python (Join-Path $python.FullName 'python.exe') --extra rekordbox --extra test
if ($LASTEXITCODE) { throw 'Dependency installation failed' }
& (Join-Path $root '.venv/Scripts/python.exe') -m onset_service doctor
