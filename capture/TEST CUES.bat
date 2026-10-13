@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - listen to the recorder's sounds
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: the recorder's own sounds
echo ============================================================
echo.
echo While you record, the recorder makes a short sound for each step, so
echo you always know what it is doing even if you cannot see the screen.
echo This plays all of them, one at a time. Before each sound, this window
echo says in words what it is about to play.
echo.
echo   - the COUNTDOWN: one very short high beep, once a second
echo   - the START: three rising notes. Recording begins when they end.
echo   - a PAUSE: two falling notes
echo   - a RESUME: two rising notes
echo   - the STOP: three falling notes, the last one low and long
echo   - an ERROR: two low, rough buzzes
echo.
echo None of these sounds is ever recorded. They only play while the
echo recording is not running: during the countdown, while paused, and after
echo the stop.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > cue-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo HOW LOUD? Type a number from 1 to 100 and press Enter, or just press
echo Enter for 70. The sounds also follow your Windows volume.
set "VOL="
set /p VOL=Volume: 
if "%VOL%"=="" set VOL=70

echo.
echo Playing now. It takes about 12 seconds.
echo.
"%~dp0target\release\openstage-capture.exe" cue-test --plain --volume %VOL% --out "%~dp0cue-test-output" > cue-test-log.txt 2>&1
set RESULT=%errorlevel%
type cue-test-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo Please send me the file cue-test-log.txt from this folder.
goto done

:passed
echo ------------------------------------------------------------
echo   Windows played all six sounds.
echo ------------------------------------------------------------
echo.
echo Please tell me:
echo   - Could you hear every one? Was any too quiet, too loud or unpleasant?
echo   - Could you tell them apart without looking at the lines above?
echo   - Would you like different sounds, a different length, or a spoken
echo     word as well?
echo Each sound was also saved as a file in the folder cue-test-output next
echo to this script, so you can play them again in any player.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file cue-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'cue-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
