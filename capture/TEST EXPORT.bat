@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - export a video
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: record, then export a finished video
echo ============================================================
echo.
echo This records about 12 seconds of your screen and your voice (and
echo your webcam if you choose one), then turns the recording into a
echo finished MP4 file. To prove the drawing works, the video gets one
echo of everything added to it:
echo   - a title in the top left,
echo   - a red box, a yellow ellipse and a green arrow,
echo   - a blurred patch and a blacked-out patch on the right,
echo   - a zoom into the top left quarter near the end,
echo   - and the webcam in the bottom right corner, if you chose one.
echo.
echo PRIVACY: this records your screen and your voice. Everything is
echo saved only on this computer, in the folder export-test-output next
echo to this file. Nothing is sent anywhere.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "export-test-log.txt" del "export-test-log.txt"
call "%~dp0cargo-run.bat" "export-test-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo These cameras were found:
"%~dp0target\release\openstage-capture.exe" cameras --plain
echo.
echo WEBCAM? Type a camera's number and press Enter to include it, or
echo just press Enter to leave the webcam out.
set "CAMERA="
set /p CAMERA=Camera: 
if "%CAMERA%"=="" set "CAMERA=none"
echo.
echo WHAT TO DO: after the countdown, keep something moving on the
echo screen for about 12 seconds (scroll a page, drag a window) and
echo say a few sentences. Put some words on the screen on the right
echo side, so you can see the blur and the blackout hide them.
echo It starts in 8 seconds, with a countdown.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0export-test-output" rmdir /s /q "%~dp0export-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0export-test-output\lesson.capture" --source screen --camera "%CAMERA%" --microphone default --output none --countdown-ms 3000 --plain --demo-script "stop@15000" > export-test-log.txt 2>&1
set RECORD_RESULT=%errorlevel%
echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type export-test-log.txt
echo.
if not "%RECORD_RESULT%"=="0" goto record_failed

echo ------------------------------------------------------------
echo   Now exporting the video. This takes a little while: the
echo   program draws every picture itself. How far it has got is
echo   written to the file export-log.txt in this folder as it goes.
echo   To stop it, press Ctrl+C; if Windows asks Terminate batch job,
echo   answer N.
echo ------------------------------------------------------------
"%~dp0target\release\openstage-capture.exe" render --project "%~dp0export-test-output\lesson.capture" --take 1 --demo --out "%~dp0export-test-output\demo-video.mp4" --plain > export-log.txt 2>&1
set RESULT=%errorlevel%
type export-log.txt
echo.

set VIDEO=%~dp0export-test-output\demo-video.mp4
if exist "%VIDEO%" (
  echo Opening the video it made.
  start "" "%VIDEO%"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   The export did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo Please send me the files export-test-log.txt and export-log.txt
echo from this folder.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The video was made and Windows read it back.
echo ------------------------------------------------------------
echo.
echo Please send me everything above, and tell me what you saw in
echo the video:
echo   - the title, in the top left, readable and not cut off?
echo   - the red box, the yellow ellipse and the green arrow, drawn
echo     where you would expect, with smooth edges?
echo   - the blurred patch: is the text under it unreadable?
echo   - the blacked-out patch: solid black?
echo   - the zoom near the end: smooth, into the top left quarter,
echo     with the blur and the blackout still hiding the same words?
echo   - the webcam in the bottom right corner, the right way up?
echo   - your voice, and does it match what you see?
echo   - is the picture sharp, not blurry or blocky?
echo.
echo To test cancelling: run this again, and press Ctrl+C while it is
echo exporting. It should say the export was cancelled, and leave no
echo demo-video.mp4 behind (and no demo-video.mp4.part file).
goto done

:record_failed
echo ------------------------------------------------------------
echo   The recording did not work, so nothing was exported.
echo ------------------------------------------------------------
echo.
echo The lines above say why. If it says there is no microphone, plug
echo one in. Please send me the file export-test-log.txt from this
echo folder if it is not clear.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file export-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'export-test-log.txt' -Tail 30"

:done
echo.
pause
