@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - record the webcam
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: record the webcam
echo ============================================================
echo.
echo This records about 9 seconds of your screen, your voice and your
echo webcam, with a pause in the middle, then opens the webcam video.
echo The webcam goes into a file of its own, webcam.mp4, next to the
echo screen video. It is never burned into the screen video: where it
echo appears is decided later, when you edit.
echo.
echo First it lists the cameras, with a number for each. You choose one
echo by its number, or by typing part of its name. If you only press
echo Enter, the first one is used.
echo.
echo PRIVACY: this records your face and your voice. It is saved only on
echo this computer, in the folder webcam-test-output next to this file.
echo Nothing is sent anywhere. Your camera's light comes on while it
echo records. A camera is never used unless you ask for it.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > webcam-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo These cameras were found:
echo.
"%~dp0target\release\openstage-capture.exe" cameras --plain
if errorlevel 1 goto no_camera
echo.
echo WHICH CAMERA? Type its number, or part of its name, and press Enter.
echo Press Enter alone for the first one.
set "CHOICE="
set /p CHOICE=Camera: 
if "%CHOICE%"=="" set "CHOICE=default"
echo.
echo WHAT TO DO: after the countdown, look at the camera and say a
echo sentence. Wave one hand in front of it, and clap once. The
echo recorder pauses by itself after about 4 seconds, for 2 seconds,
echo and carries on. Keep still during the pause, then wave again.
echo It starts in 8 seconds, with a countdown.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0webcam-test-output" rmdir /s /q "%~dp0webcam-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0webcam-test-output\lesson.capture" --source screen --camera "%CHOICE%" --microphone default --output none --countdown-ms 3000 --plain --demo-script "pause@7000;resume@9000;stop@14000" > webcam-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type webcam-test-log.txt
echo.

set VIDEO=%~dp0webcam-test-output\lesson.capture\takes\take-001\webcam.mp4
if exist "%VIDEO%" (
  echo Opening the webcam video it made. It should show you, the right
  echo way up, as the camera sees you.
  start "" "%VIDEO%"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo If the lines above say the camera cannot be started, they say why
echo and what to change. Please send me the file webcam-test-log.txt
echo from this folder if it is not clear.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder made a webcam video beside the screen video.
echo ------------------------------------------------------------
echo.
echo The numbers above include one line about the webcam. Please send
echo me everything above, and tell me what the webcam video showed:
echo   - you, the right way up and not upside down?
echo   - not mirrored? Hold up a page with writing on it: it should read
echo     the normal way, not backwards. Mirroring is only for a preview.
echo   - smooth, with no freezing or jumping?
echo   - the right size, not stretched?
echo   - after the pause, carrying on from where you were, with no jump?
echo.
echo The screen video and your voice are in the same folder:
echo webcam-test-output\lesson.capture\takes\take-001
echo.
echo To check that the webcam lines up with your voice I need the editor
echo to play them together. For now the line about the webcam says how
echo long each picture was on its way, and that is what the recorder
echo corrects for.
goto done

:no_camera
echo.
echo No camera is ready, so nothing was recorded. The line above says
echo why. If a camera is plugged in, check that no other program is
echo using it, and that Windows allows desktop programs to use cameras:
echo Settings, Privacy and security, Camera.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file webcam-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'webcam-test-log.txt' -Tail 30"

:done
echo.
pause
