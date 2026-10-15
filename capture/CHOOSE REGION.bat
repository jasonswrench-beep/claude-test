@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - choose the part of the screen to record
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: choose the part of the screen to record
echo ============================================================
echo.
echo The chosen display goes dim, and a clear rectangle appears with a
echo handle on each corner and on the middle of each side.
echo.
echo WITH THE MOUSE: drag inside the rectangle to move it, drag a handle
echo to resize it, or drag anywhere on the dim part to draw a new one.
echo.
echo WITH THE KEYBOARD OR A SCREEN READER: a small window called Choose
echo the part of the screen to record has four boxes: Left, Top, Width and
echo Height, in pixels. Type in them and the rectangle moves. Tab goes
echo from box to box and then to the buttons. Enter uses the region.
echo Escape cancels. The last box says in words what is chosen, or what
echo is wrong with it. You never have to use the mouse.
echo.
echo Nothing is recorded now. The numbers you choose are saved in the file
echo chosen-region.txt next to this one, and TEST REGION.bat offers them.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "choose-region-log.txt" del "choose-region-log.txt"
call "%~dp0cargo-run.bat" "choose-region-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo WHICH DISPLAY? Type its number and press Enter, or just press Enter
echo for display 1, the main one.
set "DISPLAY_NUMBER="
set /p DISPLAY_NUMBER=Display number: 
if "%DISPLAY_NUMBER%"=="" set DISPLAY_NUMBER=1
echo.
echo Opening the picker now. It may take a moment to appear.
echo.

"%~dp0target\release\openstage-capture.exe" pick-region --display %DISPLAY_NUMBER% --plain --out "%~dp0chosen-region.txt" > choose-region-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the picker said
echo ------------------------------------------------------------
type choose-region-log.txt
echo.

if "%RESULT%"=="0" goto chosen
echo ------------------------------------------------------------
echo   No region was chosen. See the lines above.
echo ------------------------------------------------------------
echo.
echo If you closed the picker on purpose, nothing is wrong. If it did not
echo open, or something looked wrong, please tell me what you saw and send
echo me the file choose-region-log.txt from this folder.
goto done

:chosen
echo ------------------------------------------------------------
echo   Done. The region is saved in chosen-region.txt
echo ------------------------------------------------------------
echo.
echo Next, run TEST REGION.bat. It offers this region, records 8 seconds of
echo it and opens the video.
echo.
echo Please tell me how the picker worked: could you see the rectangle and
echo the handles, did dragging feel right, did the four numbers follow
echo the mouse, and could you do it all from the keyboard.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file choose-region-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'choose-region-log.txt' -Tail 30"

:done
echo.
pause
