Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$target = Join-Path $scriptDir 'windows\health-check.ps1'

if (-not (Test-Path -LiteralPath $target)) {
    throw "Missing health-check: $target"
}

& $target @args
exit $LASTEXITCODE
