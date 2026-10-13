@echo off
setlocal
title OpenStage Capture - build
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: build the program
echo ============================================================
echo.
echo This turns the source code into the program openstage-capture.exe.
echo The first time it can take several minutes. Later runs are faster.
echo You do not need to do anything while it works. A full log is saved
echo in this folder as build-log.txt.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Found Rust:
cargo --version
echo.
echo Building. Please wait...
cargo build --release --workspace > build-log.txt 2>&1
if errorlevel 1 goto failed

echo.
echo ------------------------------------------------------------
echo   SUCCESS. The program was built.
echo ------------------------------------------------------------
echo.
echo It is here:
echo   %~dp0target\release\openstage-capture.exe
echo.
echo Quick check that it starts and can say what it can record:
echo.
"%~dp0target\release\openstage-capture.exe" probe
echo.
echo Right now only a built-in test source exists. Recording your real
echo screen and microphone is the next phase, and is not built yet.
goto done

:no_rust
echo ------------------------------------------------------------
echo   Rust is not installed on this computer yet.
echo ------------------------------------------------------------
echo.
echo Rust is the free tool that builds this program. To install it:
echo   1. Go to  https://rustup.rs
echo   2. Download and run rustup-init.exe
echo   3. Press Enter to accept the default choices. If it offers to
echo      install the Microsoft C++ Build Tools, say yes.
echo   4. When it finishes, close this window and double-click
echo      BUILD CAPTURE.bat again.
goto done

:failed
echo.
echo ------------------------------------------------------------
echo   The build did NOT work.
echo ------------------------------------------------------------
echo.
echo Nothing on your computer was changed outside this folder.
echo Please send me the file build-log.txt from this folder, or copy the
echo last lines shown below. Those lines usually say what went wrong.
echo.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
powershell -NoProfile -Command "Get-Content -LiteralPath 'build-log.txt' -Tail 25"

:done
echo.
pause
endlocal
