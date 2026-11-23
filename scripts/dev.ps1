[CmdletBinding()]
param(
    [switch]$CoreOnly
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'common.ps1')

$previousRoot = $env:FTW_DEV_ROOT
try {
    $env:FTW_DEV_ROOT = Join-Path $root '.ftw-dev\workbench'
    Push-Location -LiteralPath $root
    try {
        Invoke-Checked cargo @('build', '--locked', '-p', 'ftw-core', '-p', 'ftw-mcp')
        if ($CoreOnly) {
            $metadata = cargo metadata --no-deps --format-version 1
            if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
            $target = ($metadata | ConvertFrom-Json).target_directory
            $core = Join-Path $target 'debug\ftw-core.exe'
            Invoke-Checked $core @('serve', '--dev-root', $env:FTW_DEV_ROOT)
            return
        }

        Set-Location -LiteralPath (Join-Path $root 'apps\desktop')
        if (-not (Test-Path -LiteralPath 'node_modules\@tauri-apps\cli')) {
            Invoke-Checked npm.cmd @('ci', '--no-audit', '--no-fund')
        }
        Invoke-Checked npm.cmd @('run', 'tauri', '--', 'dev', '--', '--locked')
    } finally {
        Pop-Location
    }
} finally {
    $env:FTW_DEV_ROOT = $previousRoot
}
