@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - record your screen and sound
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: a real recording, with sound
echo ============================================================
echo.
echo This is a test of the real recorder, the same program that OpenStage
echo will use. It follows a script, so you only watch and talk:
echo.
echo   - a 3 second countdown, then it records your main screen,
echo     your MICROPHONE, and the SOUND YOUR COMPUTER IS PLAYING
echo   - a marker is added about 2 seconds in
echo   - it PAUSES for about 2 seconds, then RESUMES
echo   - a second marker is added
echo   - it STOPS, finishes the video file, and checks everything
echo.
echo Time spent paused is not recorded, so the finished video should be
echo about 9 seconds long, not the 14 seconds this takes.
echo.
echo PRIVACY: the recording shows your screen and holds your voice and
echo whatever your computer played. It is saved only on this computer, in
echo the folder record-test-output next to this file. Nothing is sent
echo anywhere. You can delete the folder afterwards.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > record-test-log.txt 2>&1
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
echo WHICH SOUND OUTPUT? This is where the sound you want to record plays,
echo for example your speakers or headphones. Type its number and press
echo Enter, or just press Enter for the default. Not sure? Close this window
echo and double-click FIND SOUND OUTPUT.bat first. To record no computer
echo sound at all, type none.
set "OUT="
set /p OUT=Sound output number: 
echo.
echo BALANCE, optional. The mixed sound inside the video can make one sound
echo quieter or louder. Type a number of decibels, for example -10 to make
echo the computer's sound quieter, or just press Enter to leave it as heard.
echo About 6 decibels is half the volume. The separate files mic.wav and
echo system.wav are never changed, so you can always start again from them.
set "SYSGAIN="
set /p SYSGAIN=Computer sound change in decibels: 
if "%SYSGAIN%"=="" set SYSGAIN=0
set "MICGAIN="
set /p MICGAIN=Microphone change in decibels: 
if "%MICGAIN%"=="" set MICGAIN=0
echo.
echo WHAT TO DO:
echo   - Keep something moving on your main screen the whole time, for
echo     example drag a window around.
echo   - Start some sound playing, for example a video or a song, and keep
echo     it playing. If you can, wear headphones, so your microphone does not
echo     also hear the speakers and make an echo.
echo   - SAY a sentence out loud while it records, such as: Testing, one two
echo     three, this is my voice.
echo.
echo It starts in 8 seconds.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0record-test-output" rmdir /s /q "%~dp0record-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0record-test-output\lesson.capture" --source screen --microphone "%MIC%" --output "%OUT%" --mic-gain-db %MICGAIN% --system-gain-db %SYSGAIN% --countdown-ms 3000 --plain --demo-script "marker:first marker@5000;pause@7000;resume@9000;marker:after the pause@10000;stop@14000" > record-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type record-test-log.txt
echo.

set VIDEO=%~dp0record-test-output\lesson.capture\takes\take-001\screen.mp4
if exist "%VIDEO%" (
  echo Opening the video it made. It should show your screen, the right way
  echo up, with no gap where the pause was, and you should hear yourself and
  echo the computer's sound together. The folder next to it also holds
  echo mic.wav with only you in it, and system.wav with only the computer.
  start "" "%VIDEO%"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo Please send me the file record-test-log.txt from this folder (it is
echo plain text and contains no pictures), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder made a real recording and Windows can play it.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed, what the video looked like, and what you
echo heard: your voice, the computer's sound, whether they were well
echo balanced, and whether they stayed together with the picture.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file record-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'record-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
