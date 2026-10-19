$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = [IO.Path]::GetFullPath((Join-Path $root '.artifacts/runtime'))
if (-not $target.StartsWith([IO.Path]::GetFullPath((Join-Path $root '.artifacts')) + [IO.Path]::DirectorySeparatorChar)) { throw 'Runtime target is outside workspace artifacts' }
$python = Get-ChildItem -LiteralPath (Join-Path $root '.managed-python') -Directory | Where-Object Name -Like 'cpython-3.11*' | Select-Object -First 1
if (-not $python) { throw 'Run scripts/bootstrap.ps1 first' }
New-Item -ItemType Directory -Force -Path $target | Out-Null
Get-ChildItem -LiteralPath $python.FullName | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $target -Recurse -Force }
$packages = Join-Path $root '.venv/Lib/site-packages'
Get-ChildItem -LiteralPath $packages | Where-Object { $_.Name -notlike '*editable*' -and $_.Name -notlike 'onset_service-*.dist-info' } | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $target 'Lib/site-packages') -Recurse -Force }
& (Join-Path $target 'python.exe') -c "import numpy, scipy, soundfile, librosa, pyrekordbox; print('Packaged runtime ready')"
if ($LASTEXITCODE) { throw 'Packaged runtime verification failed' }
