@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - screen capture test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: try capturing your screen
echo ============================================================
echo.
echo This checks that Windows will let the program see your screen,
echo and how smoothly the pictures arrive. It does NOT make a recording
echo yet. It saves two still pictures of your main screen so you can
echo look at them.
echo.
echo PRIVACY: those two pictures show whatever was on your screen. They
echo are saved only on this computer, in the folder screen-test-output
echo next to this file. Nothing is sent anywhere. You do not need to
echo send them to anyone, and you can delete the folder when you are done.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "screen-test-log.txt" del "screen-test-log.txt"
call "%~dp0cargo-run.bat" "screen-test-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO: for the next 8 seconds the program watches your main
echo screen. Please keep something moving the whole time, for example
echo drag a window around, or play a video. A still screen sends very
echo few pictures, which is normal but tells us nothing.
echo.
echo It starts in 5 seconds.
timeout /t 5 /nobreak >nul
echo Capturing now. Keep a window moving...
echo.

"%~dp0target\release\openstage-capture.exe" screen-test --plain --seconds 8 --out "%~dp0screen-test-output" > screen-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type screen-test-log.txt
echo.

if exist "%~dp0screen-test-output\last.bmp" (
  echo Opening the last picture. It should show your main screen as it
  echo looked at the end of the test. The mouse pointer is NOT in it, on
  echo purpose: the pointer will be recorded separately so it can be made
  echo bigger or highlighted when editing. Tell me if the picture is
  echo black, shows the wrong screen, or looks scrambled.
  start "" "%~dp0screen-test-output\last.bmp"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one check found a problem. See the lines above.
echo ------------------------------------------------------------
echo.
echo Please send me the file screen-test-log.txt from this folder (it
echo is plain text and contains no pictures), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. Windows let the program capture your screen smoothly.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed and whether the picture looked right.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file screen-test-log.txt from this folder.
echo.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
powershell -NoProfile -Command "Get-Content -LiteralPath 'screen-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
