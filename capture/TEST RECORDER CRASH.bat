@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - recorder crash test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: what survives a crash?
echo ============================================================
echo.
echo A recording must not be lost because the computer lost power or a
echo program crashed. This starts the real recorder, with sound, records
echo about 12 seconds (with a 2 second pause in the middle), then STOPS THE
echo RECORDER WITHOUT WARNING, the way a crash or a power cut would. Then
echo it repairs the recording the way the program does for you, and checks
echo the video, the sound inside it, and the separate sound files.
echo.
echo Only the recorder's own process is stopped. Nothing else on your
echo computer is touched.
echo.
echo PRIVACY: the test recording shows your screen and holds your voice. It
echo is saved only on this computer, in the folder recorder-crash-output
echo next to this file. Nothing is sent anywhere. You can delete the folder
echo afterwards.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > recorder-crash-log.txt 2>&1
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
echo Enter for the default. To record no computer sound, type none.
set "OUT="
set /p OUT=Sound output number: 
echo.
echo WHAT TO DO:
echo   - Keep something moving on your main screen the whole time, for
echo     example drag a window around.
echo   - Keep some sound playing, and SAY something out loud now and then.
echo.
echo It starts in 8 seconds and takes about 30.
timeout /t 8 /nobreak >nul
echo Recording, then crashing it on purpose, then repairing...
echo.

"%~dp0target\release\openstage-capture.exe" recorder-crash-test --plain --microphone "%MIC%" --output "%OUT%" --out "%~dp0recorder-crash-output" > recorder-crash-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type recorder-crash-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo That is useful, not a disaster. Please send me the file
echo recorder-crash-log.txt from this folder, and the file
echo recorder-crash-output\recorder-diagnostics.txt if there is one. Both
echo are plain text and contain no pictures or sound.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. A crashed recording came back, with its sound.
echo ------------------------------------------------------------
echo.
echo Please send me the lines above anyway. The exact numbers matter.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file recorder-crash-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'recorder-crash-log.txt' -Tail 30"

:done
echo.
pause
endlocal
