@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - record part of the screen
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: record only part of the screen
echo ============================================================
echo.
echo This records 8 seconds of just a RECTANGLE of one display, then opens
echo the video so you can see that only that rectangle is in it. The
echo rectangle is given as four numbers, measured in pixels from the top
echo left corner of the display:
echo.
echo   how far from the LEFT, how far from the TOP, the WIDTH, the HEIGHT
echo.
echo For example 100,200,1280,720 is a rectangle 1280 wide and 720 tall,
echo starting 100 pixels from the left edge and 200 from the top.
echo.
echo To choose the rectangle by dragging it on the screen, run CHOOSE
echo REGION.bat first. If you did, this script offers that choice.
echo.
echo While it records, a thin orange frame is shown just OUTSIDE the
echo rectangle from the countdown until the end, so you can see what is
echo being kept. It is not in the video. Windows also draws its own thin
echo frame around the WHOLE display to show that the screen is being
echo recorded; that one is Windows, not this program, and is not in the
echo video either.
echo.
echo PRIVACY: this records part of your screen. It is saved only on this
echo computer, in the folder region-test-output next to this file. Nothing
echo is sent anywhere. No sound is recorded in this test.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > region-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo WHICH DISPLAY? Type its number and press Enter, or just press Enter
echo for display 1, the main one.
set "DISPLAY_NUMBER="
set /p DISPLAY_NUMBER=Display number: 
if "%DISPLAY_NUMBER%"=="" set DISPLAY_NUMBER=1
echo.
set "DEFAULT_AREA=0,0,640,360"
set "WHERE=the top left corner of the display, which fits any display"
if exist "%~dp0chosen-region.txt" set /p DEFAULT_AREA=<"%~dp0chosen-region.txt"
if exist "%~dp0chosen-region.txt" set "WHERE=the region you chose with CHOOSE REGION.bat"
echo WHICH RECTANGLE? Type four numbers separated by commas and press
echo Enter, or just press Enter for %DEFAULT_AREA%, %WHERE%.
set "AREA="
set /p AREA=Rectangle: 
if "%AREA%"=="" set AREA=%DEFAULT_AREA%
echo.
echo WHAT TO DO: put a window with something you will recognise (a
echo document, a web page) inside that rectangle and keep something moving
echo in it, for example scroll. It starts in 8 seconds, with a countdown.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0region-test-output" rmdir /s /q "%~dp0region-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0region-test-output\lesson.capture" --source screen --display %DISPLAY_NUMBER% --region "%AREA%" --microphone none --output none --countdown-ms 3000 --plain --demo-script "stop@11000" > region-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type region-test-log.txt
echo.

set VIDEO=%~dp0region-test-output\lesson.capture\takes\take-001\screen.mp4
if exist "%VIDEO%" (
  echo Opening the video it made. It should show only the rectangle you
  echo chose, nothing else from the screen.
  start "" "%VIDEO%"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo If the lines above say the rectangle cannot be used, they say why and
echo what to change. Please send me the file region-test-log.txt from this
echo folder if it is not clear.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder made a video of just that rectangle.
echo ------------------------------------------------------------
echo.
echo Please tell me what the video showed: only the rectangle, in the
echo right place, not stretched, with nothing missing or cut off.
echo.
echo And did you see a thin orange frame just outside the rectangle from
echo the countdown until the end? Was it easy to see on your screen?
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file region-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'region-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
