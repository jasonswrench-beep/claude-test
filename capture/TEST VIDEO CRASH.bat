@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - crash safety test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: what survives a crash?
echo ============================================================
echo.
echo A recording is only trustworthy if a crash or a power cut does not
echo lose it. This test checks that on YOUR computer:
echo.
echo   1. The program starts recording a video of your main screen.
echo   2. After about 8 seconds it is stopped WITHOUT WARNING, the way a
echo      crash or a power cut would stop it. Only this test's own
echo      recording program is stopped. Nothing else on your computer is
echo      touched.
echo   3. The program then looks at what was saved, repairs the file, and
echo      tells you how many seconds survived.
echo.
echo PRIVACY: the saved video shows whatever was on your screen. It stays
echo on this computer, in the folder video-crash-output next to this file.
echo Nothing is sent anywhere. You can delete the folder when you are done.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "video-crash-log.txt" del "video-crash-log.txt"
call "%~dp0cargo-run.bat" "video-crash-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO: keep something moving on your main screen for the next
echo 15 seconds, for example drag a window around. There is nothing else
echo to do. It starts in 5 seconds.
timeout /t 5 /nobreak >nul
echo Recording, then stopping without warning. Keep a window moving...
echo.

"%~dp0target\release\openstage-capture.exe" video-crash-test --plain --seconds-before-kill 8 --out "%~dp0video-crash-output" > video-crash-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type video-crash-log.txt
echo.

if exist "%~dp0video-crash-output\test_fragmented.mp4" (
  echo Opening the repaired video. It should show the first few seconds of
  echo your screen and stop where the recording was cut off. It has no
  echo sound, on purpose.
  start "" "%~dp0video-crash-output\test_fragmented.mp4"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo That is useful, not a disaster: it tells us how to make saving safer.
echo Please send me the file video-crash-log.txt from this folder (it is
echo plain text and contains no pictures), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. After a sudden stop, almost all of the video survived.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed and what the repaired video looked like.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file video-crash-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'video-crash-log.txt' -Tail 30"

:done
echo.
pause
endlocal
