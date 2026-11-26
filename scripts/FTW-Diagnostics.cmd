@echo off
setlocal
set "FTW_REPORT=%TEMP%\FTW-diagnostics-%RANDOM%-%RANDOM%.json"
set "FTW_APP_DIR=%~dp0"
if not exist "%FTW_APP_DIR%ftw-core.exe" goto fallback
"%FTW_APP_DIR%ftw-core.exe" diagnostics > "%FTW_REPORT%" 2>nul
if not errorlevel 1 goto saved

:fallback
powershell.exe -NoProfile -NonInteractive -Command "$binaries = foreach ($name in 'workbench.exe','ftw-core.exe','ftw-mcp.exe') { @{ name=$name; present=[IO.File]::Exists((Join-Path $env:FTW_APP_DIR $name)) } }; @{ format_version=1; generated_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); collection_status='core_report_unavailable'; windows_version=[Environment]::OSVersion.Version.ToString(); binaries=@($binaries); events=@() } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $env:FTW_REPORT -Encoding UTF8 -ErrorAction Stop"
if errorlevel 1 goto failed

:saved
echo FTW diagnostics saved to:
echo "%FTW_REPORT%"
echo Open and review the report before sharing it.
pause
exit /b 0

:failed
echo Unable to save the FTW diagnostics report to the temporary directory.
pause
exit /b 1
