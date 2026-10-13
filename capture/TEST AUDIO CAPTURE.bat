@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - sound test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: try recording sound
echo ============================================================
echo.
echo This checks that Windows will let the program hear TWO things on
echo your computer, and saves what it heard so you can listen:
echo.
echo   1. your MICROPHONE (so it can record your voice)
echo   2. the SOUND YOUR COMPUTER IS PLAYING (so it can record a video,
echo      a program, or a screen reader that is speaking)
echo.
echo This test records sound only: no picture and no video.
echo.
echo PRIVACY: the sound files contain your voice and whatever your computer
echo was playing. They are saved only on this computer, in the folder
echo audio-test-output next to this file. Nothing is sent anywhere. You
echo can delete the folder afterwards.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > audio-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO (before it starts):
echo   - Start some sound playing on your computer, for example a video,
echo     a song, or a talking web page. Keep it playing.
echo   - If you can, use headphones, so your microphone does not also hear
echo     the speakers.
echo.
echo WHAT TO DO (during the 8 seconds):
echo   - SAY a sentence out loud, clearly, such as "Testing, one two three,
echo     this is my voice."
echo.
echo It starts in 8 seconds.
timeout /t 8 /nobreak >nul
echo Listening now. Say your sentence...
echo.

"%~dp0target\release\openstage-capture.exe" audio-test --plain --seconds 8 --out "%~dp0audio-test-output" > audio-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type audio-test-log.txt
echo.

if exist "%~dp0audio-test-output\both.wav" (
  echo Playing both.wav now. You should hear YOURSELF and the sound your
  echo computer was playing, mixed together. The folder also holds mic.wav
  echo (only you) and system.wav (only the computer's sound).
  start "" "%~dp0audio-test-output\both.wav"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo That is useful, not a disaster. Please send me the file
echo audio-test-log.txt from this folder (it is plain text and contains
echo no sound), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. Windows let the program hear both, and the clocks agree.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed and what you heard when the file played.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file audio-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'audio-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
