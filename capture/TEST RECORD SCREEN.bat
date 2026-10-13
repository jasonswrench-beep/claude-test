@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - record your screen
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: a real recording, start to finish
echo ============================================================
echo.
echo This is the first test of the real recorder, the same program that
echo OpenStage will use. It follows a script, so you only watch:
echo.
echo   - a 3 second countdown, then it records your main screen
echo   - a marker is added about 2 seconds in
echo   - it PAUSES for about 2 seconds, then RESUMES
echo   - a second marker is added
echo   - it STOPS, finishes the video file, and checks it
echo.
echo Time spent paused is not recorded, so the finished video should be
echo shorter than the 9 seconds this takes. There is NO SOUND yet.
echo.
echo PRIVACY: the recording shows whatever was on your screen. It is saved
echo only on this computer, in the folder record-test-output next to this
echo file. Nothing is sent anywhere. You can delete the folder afterwards.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > record-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO: keep something moving on your main screen the whole
echo time, for example drag a window around. It starts in 5 seconds.
timeout /t 5 /nobreak >nul
echo.

if exist "%~dp0record-test-output" rmdir /s /q "%~dp0record-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0record-test-output\lesson.capture" --source screen --countdown-ms 3000 --plain --demo-script "marker:first marker@5000;pause@7000;resume@9000;marker:after the pause@10000;stop@12000" > record-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type record-test-log.txt
echo.

set VIDEO=%~dp0record-test-output\lesson.capture\takes\take-001\screen.mp4
if exist "%VIDEO%" (
  echo Opening the video it made. It should show your screen, the right way
  echo up, with no gap where the pause was. It has no sound, on purpose.
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
echo Please tell me it passed and what the video looked like.
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
