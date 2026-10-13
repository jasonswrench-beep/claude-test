@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - video test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: try making a video of your screen
echo ============================================================
echo.
echo This makes an 8 second MP4 video of your main screen with Windows'
echo own video tools, then reports how smoothly it went and how hard
echo your computer worked. There is NO SOUND yet: sound comes later.
echo.
echo PRIVACY: the video shows whatever was on your screen. It is saved
echo only on this computer, in the folder video-test-output next to this
echo file. Nothing is sent anywhere. You do not need to send it to anyone,
echo and you can delete the folder when you are done.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > video-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO: for the next 8 seconds the program records your main
echo screen. Please keep something moving the whole time, for example
echo drag a window around, or play a video. Then, for the best test of
echo your computer, do it again with a busy screen, such as a video
echo playing full screen.
echo.
echo It starts in 5 seconds.
timeout /t 5 /nobreak >nul
echo Recording now. Keep a window moving...
echo.

"%~dp0target\release\openstage-capture.exe" video-test --plain --seconds 8 --out "%~dp0video-test-output" > video-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type video-test-log.txt
echo.

if exist "%~dp0video-test-output\test.mp4" (
  echo Opening the video in your normal video player. Please check four things:
  echo   1. It shows your main screen.
  echo   2. It is the right way up, not upside down or mirrored.
  echo   3. The colors look right and it plays smoothly.
  echo   4. It lasts about 8 seconds. It has no sound, on purpose.
  start "" "%~dp0video-test-output\test.mp4"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo Please send me the file video-test-log.txt from this folder (it
echo is plain text and contains no pictures), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. Windows made the video and your computer kept up.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed and what you saw in the video.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file video-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'video-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
