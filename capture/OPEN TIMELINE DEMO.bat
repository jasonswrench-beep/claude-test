@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - clip list demo
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: the clip list (editing demo)
echo ============================================================
echo.
echo This opens a demo of the clip list in your web browser. It shows a
echo made-up lesson cut into clips, which you can split, trim, delete,
echo join, rename and move with the keyboard.
echo.
echo It uses made-up data and changes nothing on your computer. It does
echo not record anything, and nothing is sent anywhere. The page is
echo served only to this computer, and stops when you close this window.
echo.

call "%~dp0rust-setup.bat" node
if errorlevel 1 goto no_node

for /f "delims=" %%v in ('node --version') do set NODE_VERSION=%%v
echo Using Node.js %NODE_VERSION%
echo.
echo Starting. Your browser will open by itself.
echo When you are done, close this window.
echo.
node "%~dp0..\capture-timeline\demo\serve.mjs" --open
if errorlevel 1 goto failed
goto done

:no_node
echo Node.js was not found on this computer. The demo needs it, version 22.13
echo or newer. It is free: https://nodejs.org (choose the LTS version).
echo If Node.js is installed somewhere else, set the variable OPENSTAGE_RUST_ENV
echo to a PowerShell file that puts it on the PATH, then run this again.
goto done

:failed
echo.
echo The demo stopped. What it said is above. If it is not clear, please
echo send me what this window shows.
goto done

:done
echo.
pause
