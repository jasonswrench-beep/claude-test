@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - picture and sound timing
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: do picture and sound stay together?
echo ============================================================
echo.
echo When Windows hands the recorder a picture or a piece of sound, it has
echo already spent some time getting it there. If the picture is usually
echo 30 thousandths of a second late and the sound 90, a recording would
echo have them 60 thousandths of a second out of step. Under about 40 is
echo not noticeable.
echo.
echo This measures that on your computer for 10 seconds. It watches your
echo main screen, your microphone, and the sound your computer is playing,
echo and reports how late each one arrives.
echo.
echo PRIVACY: nothing is saved and nothing is sent anywhere. It only
echo measures timing, not what is on the screen or what is said.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > timing-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo These are the microphones and sound outputs on this computer:
echo.
"%~dp0target\release\openstage-capture.exe" audio-devices --plain
echo.
echo WHICH MICROPHONE? Type its number and press Enter, or just press Enter
echo for the default.
set "MIC="
set /p MIC=Microphone number: 
echo.
echo WHICH SOUND OUTPUT? Type its number and press Enter, or just press
echo Enter for the default. Not sure? Close this window and double-click
echo FIND SOUND OUTPUT.bat first.
set "OUT="
set /p OUT=Sound output number: 
echo.
echo WHAT TO DO:
echo   - Keep something moving on your main screen the whole time, for
echo     example drag a window around.
echo   - Keep some sound playing, for example a video or a song.
echo   - SAY something out loud, and keep talking or humming.
echo.
echo It starts in 8 seconds and takes 10.
timeout /t 8 /nobreak >nul
echo Measuring now...
echo.

"%~dp0target\release\openstage-capture.exe" timing-test --plain --seconds 10 --microphone "%MIC%" --output "%OUT%" > timing-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type timing-test-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo That is useful, not a disaster. Please send me the file
echo timing-test-log.txt from this folder (it is plain text), or copy
echo the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. Picture and sound reach the recorder together.
echo ------------------------------------------------------------
echo.
echo Please send me the lines above anyway. The exact numbers help.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file timing-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'timing-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
