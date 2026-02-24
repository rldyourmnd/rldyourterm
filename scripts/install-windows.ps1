Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$target = Join-Path $scriptDir 'windows\install.ps1'

if (-not (Test-Path -LiteralPath $target)) {
    throw "Missing installer: $target"
}

& $target @args
exit $LASTEXITCODE
