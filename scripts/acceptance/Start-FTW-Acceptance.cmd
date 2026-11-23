@echo off
setlocal
set "workspace=%~dp0workspace"
if not exist "%workspace%\" mkdir "%workspace%" || goto :failed
for %%F in (workbench.exe ftw-core.exe ftw-mcp.exe portable.flag) do (
    if not exist "%workspace%\%%F" copy /y "%~dp0..\%%F" "%workspace%\%%F" >nul || goto :failed
)
start "" /d "%workspace%" "%workspace%\workbench.exe" || goto :failed
exit /b 0

:failed
echo Could not start FTW acceptance. Extract the full ZIP to a writable folder.
pause
exit /b 1
