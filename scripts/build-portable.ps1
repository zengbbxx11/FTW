[CmdletBinding()]
param(
    [switch]$ForRelease,
    [string]$SigningKey = $env:FTW_RELEASE_KEY_FILE
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'common.ps1')
$declaredVersion = (Get-Content -LiteralPath (Join-Path $root 'apps\desktop\package.json') -Raw | ConvertFrom-Json).version
$packages = Join-Path $root '.ftw-dev\packages'
$releaseDirectory = Join-Path $packages "v$declaredVersion"
if ($ForRelease -and (Test-Path -LiteralPath $releaseDirectory)) {
    throw "Release v$declaredVersion already exists. Increase the version for a new release; do not overwrite an existing release."
}

Push-Location -LiteralPath $root
try {
    Invoke-Checked cargo @('build', '--release', '--locked', '-p', 'ftw-core', '-p', 'ftw-mcp', '-p', 'ftw-updater')
    $metadata = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $workspace = $metadata | ConvertFrom-Json
    $version = ($workspace.packages | Where-Object { $_.name -eq 'ftw-core' }).version
    if ($version -ne $declaredVersion) { throw 'Cargo and desktop versions do not match.' }
    $binaries = Join-Path $workspace.target_directory 'release'

    Set-Location -LiteralPath (Join-Path $root 'apps\desktop')
    Invoke-Checked npm.cmd @('ci', '--no-audit', '--no-fund')
    Invoke-Checked npm.cmd @('run', 'tauri', '--', 'build', '--no-bundle', '--ci', '--', '--locked')
} finally {
    Pop-Location
}

$stamp = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 6)
$buildDirectory = Join-Path $root ".ftw-dev\builds\v$version\$stamp"
$destination = Join-Path $buildDirectory 'FTW-Portable'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
foreach ($binary in @('workbench.exe', 'ftw-core.exe', 'ftw-mcp.exe', 'FTW-Update.exe')) {
    Copy-Item -LiteralPath (Join-Path $binaries $binary) -Destination (Join-Path $destination $binary)
}
New-Item -ItemType File -Path (Join-Path $destination 'portable.flag') | Out-Null

foreach ($guide in @('FTW-Agent-Quickstart.txt', 'FTW-Agent-Workflows.txt')) {
    Copy-Item -LiteralPath (Join-Path $root "docs\$guide") -Destination $destination
}
Copy-Item -LiteralPath (Join-Path $root 'apps\desktop\public\THIRD_PARTY_UI.txt') -Destination $destination
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'FTW-Diagnostics.cmd') -Destination $destination

$acceptance = Join-Path $destination 'acceptance'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'acceptance') -Destination $acceptance -Recurse
$cases = Get-Content -LiteralPath (Join-Path $root 'docs\ACCEPTANCE_TESTS.md') -Encoding UTF8 -Raw
($cases -split '<!-- repository tests -->', 2)[0].TrimEnd() |
    Set-Content -LiteralPath (Join-Path $acceptance 'ACCEPTANCE_TESTS.md') -Encoding UTF8

@'
FTW portable for Windows x64

Extract the whole folder to a writable location and run workbench.exe.
WebView2 Evergreen Runtime must be available on the machine.
Keep workbench.exe, ftw-core.exe, ftw-mcp.exe and portable.flag with the data directory.

To update an older portable workbench, open that workbench, then run
FTW-Update.exe from this package and follow the prompts. After updating,
check for later versions in Workbench > System Status > Software Updates.

Current implementation and validation status:
https://github.com/zengbbxx11/FTW/blob/main/docs/PROJECT_STATUS.md
Release publication does not mean all regression and device checks were run.

For a problem report, go to System Status > Export Diagnostics in Workbench.
If the window will not open, double-click FTW-Diagnostics.cmd in this folder.
It prints the path to a local JSON file. Review the file before sharing it.

For Codex and WorkBuddy, open FTW-Agent-Quickstart.txt for setup and
FTW-Agent-Workflows.txt for ready-to-copy tasks. Both guides are in Chinese.

For acceptance checks, open acceptance\ACCEPTANCE_TESTS.md and run
acceptance\Start-FTW-Acceptance.cmd. The launcher uses its own workspace.
After the empty-state checks, import acceptance\sample-customers.csv
from Data and Audit > Customer CSV in that workspace.

Configure an email account inside FTW. SMTP and IMAP credentials use
Windows user protection. Re-enter the account credentials after a move
to another Windows user or machine.

To connect an Agent, add a local MCP stdio server with the absolute path
to ftw-mcp.exe as its command. Leave arguments empty. In a PowerShell
terminal opened in this directory, Codex can be configured with:

  codex mcp add ftw -- (Join-Path $PWD 'ftw-mcp.exe')

WorkBuddy uses the same executable in its local MCP settings.
MCP clients running as the same Windows user connect without registering an
Agent or entering a token. They share the local Agent identity. Workbench can
pause local access. Save standing send authorization separately with the
selected senders, purposes and limits.

Use FTW Data and Audit for backups and restoration. Keep mail-objects with
the matching backup database. Close Workbench and all MCP clients, and let
Core exit before moving the complete live data directory, including the
agent-send-safety.sqlite and mail-objects files. The application will take
a database backup before applying new migrations.
'@ | Set-Content -LiteralPath (Join-Path $destination 'README.txt') -Encoding UTF8

$assetDirectory = Join-Path $buildDirectory 'assets'
New-Item -ItemType Directory -Path $assetDirectory -Force | Out-Null
$zip = Join-Path $assetDirectory 'FTW-Portable-win-x64.zip'
Compress-Archive -LiteralPath $destination -DestinationPath $zip -CompressionLevel Optimal
$hash = Get-FileHash -LiteralPath $zip -Algorithm SHA256
if (-not $SigningKey) { $SigningKey = Join-Path $root '.ftw-dev\release\FTW-signing.dpapi' }
if (Test-Path -LiteralPath $SigningKey) {
    Invoke-Checked (Join-Path $binaries 'ftw-release.exe') @('sign', $zip, $version, $SigningKey)
} elseif ($ForRelease) {
    throw 'Release signing key not found. Initialize the key before publishing a signed update.'
} else {
    Write-Warning 'No signing key found; this package cannot be published as an automatic update.'
}
if ($ForRelease) {
    New-Item -ItemType Directory -Path $releaseDirectory | Out-Null
    foreach ($asset in @('FTW-Portable-win-x64.zip', 'FTW-Update.json', 'FTW-Update.json.sig')) {
        Copy-Item -LiteralPath (Join-Path $assetDirectory $asset) -Destination $releaseDirectory
    }
    Write-Output "Release assets: $releaseDirectory"
    $zip = Join-Path $releaseDirectory 'FTW-Portable-win-x64.zip'
} else {
    Write-Output "Development build: $buildDirectory"
}
Write-Output $zip
Write-Output "SHA-256 $($hash.Hash)"
