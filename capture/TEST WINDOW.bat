@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - record one window
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: record just one window
echo ============================================================
echo.
echo This records 10 seconds of ONE window, not the whole screen, then
echo opens the video. Nothing else on your screen is in it: not other
echo windows, not notification pop-ups, not the taskbar.
echo.
echo First it lists the windows that can be recorded, with a number
echo for each. Windows that are minimized or hidden, and windows with no
echo title, are not in the list. You choose one by its number, or by
echo typing part of its title.
echo.
echo PRIVACY: this records one window. It is saved only on this computer,
echo in the folder window-test-output next to this file. Nothing is sent
echo anywhere. No sound is recorded in this test.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > window-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo These windows can be recorded:
echo.
"%~dp0target\release\openstage-capture.exe" windows --plain
echo.
echo WHICH WINDOW? Type its number, or part of its title, and press Enter.
set "CHOICE="
set /p CHOICE=Window: 
if "%CHOICE%"=="" goto nothing_chosen
echo.
echo WHAT TO DO: after the countdown, keep something moving in that
echo window, for example scroll. Then try these, one after another:
echo   1. Drag another window over it. The video should still show only
echo      the window you chose, not the one in front.
echo   2. Make the window a little bigger or smaller by dragging its edge.
echo      The video keeps its size; the window should be made smaller to
echo      fit, in the middle on black, never stretched.
echo It starts in 8 seconds, with a countdown.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0window-test-output" rmdir /s /q "%~dp0window-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0window-test-output\lesson.capture" --source screen --window "%CHOICE%" --microphone none --output none --countdown-ms 3000 --plain --demo-script "stop@13000" > window-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type window-test-log.txt
echo.

set VIDEO=%~dp0window-test-output\lesson.capture\takes\take-001\screen.mp4
if exist "%VIDEO%" (
  echo Opening the video it made. It should show only the window you
  echo chose.
  start "" "%VIDEO%"
  echo.
)

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo If the lines above say the window cannot be found or used, they say
echo why and what to change. Please send me the file window-test-log.txt
echo from this folder if it is not clear.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder made a video of just that window.
echo ------------------------------------------------------------
echo.
echo Please tell me what the video showed:
echo   - only the window, with nothing else from the screen?
echo   - still only that window while another window was in front of it?
echo   - what happened when you resized it: made smaller to fit, in the
echo     middle on black, never stretched?
echo.
echo You can also run this again and CLOSE the window during the
echo recording. The recording should stop at once with a message that
echo says the window was closed, and keep what was recorded.
goto done

:nothing_chosen
echo.
echo No window was chosen, so nothing was recorded.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file window-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'window-test-log.txt' -Tail 30"

:done
echo.
pause
