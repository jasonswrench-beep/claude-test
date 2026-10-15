@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - check my computer
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: is this computer ready to record?
echo ============================================================
echo.
echo This looks at your computer and tells you, in plain words, whether it
echo can record: the Windows version, screen capture, the video and sound
echo parts an MP4 file needs, your microphone and whether Windows lets
echo programs use it, and how much disk space there is.
echo.
echo It only LOOKS. It records nothing, asks for nothing, and sends nothing
echo anywhere.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "setup-check-log.txt" del "setup-check-log.txt"
call "%~dp0cargo-run.bat" "setup-check-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
"%~dp0target\release\openstage-capture.exe" setup-check --plain --dir "%~dp0." > setup-check-log.txt 2>&1
set RESULT=%errorlevel%
type setup-check-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something needs attention. See the lines marked PROBLEM.
echo ------------------------------------------------------------
echo.
echo Each problem says what to change. After changing it, run this again.
echo If a message is not clear, please send me the file setup-check-log.txt
echo from this folder.
goto done

:passed
echo ------------------------------------------------------------
echo   This computer is ready to record.
echo ------------------------------------------------------------
echo.
echo Please send me the lines above anyway, so I can see what other
echo computers will say.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file setup-check-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'setup-check-log.txt' -Tail 30"

:done
echo.
pause
endlocal
