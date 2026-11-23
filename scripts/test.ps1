[CmdletBinding()]
param(
    [switch]$Clippy
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'common.ps1')

Push-Location -LiteralPath (Join-Path $root 'apps\desktop')
try {
    Invoke-Checked npm.cmd @('ci', '--no-audit', '--no-fund')
    Invoke-Checked npm.cmd @('run', 'lint')
    Invoke-Checked npm.cmd @('run', 'build')
    Set-Location -LiteralPath $root
    Invoke-Checked node @('--test', (Join-Path $PSScriptRoot 'test-change-keys.mjs'))
    Invoke-Checked cargo @('fmt', '--all', '--', '--check')
    Invoke-Checked cargo @('test', '--locked', '--workspace')
    Invoke-Checked cargo @('build', '--locked', '--workspace')
    $metadata = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $target = ($metadata | ConvertFrom-Json).target_directory
    Invoke-Checked node @((Join-Path $PSScriptRoot 'test-mcp.mjs'), $target)
    if ($Clippy) {
        Invoke-Checked cargo @('clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings')
    }
} finally {
    Pop-Location
}
