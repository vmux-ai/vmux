#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$SkipBuildDeps
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $Root

if (-not $SkipBuildDeps) {
    & (Join-Path $PSScriptRoot 'doctor-windows.ps1') -BuildDeps
    if (-not $?) {
        exit 1
    }
}

Remove-Item Env:CEF_PATH -ErrorAction SilentlyContinue
cargo run -p vmux_desktop --features debug
exit $LASTEXITCODE
