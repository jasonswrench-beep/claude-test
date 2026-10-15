@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - sound output switch test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: does the recording follow the sound?
echo ============================================================
echo.
echo When you plug in headphones, or pick another sound output in Windows,
echo the sound moves. The recorder is meant to move with it. This test
echo records for 70 seconds and asks you to switch the sound output twice,
echo to see whether it really does.
echo.
echo WHAT YOU WILL DO:
echo   1. Start some music or a video playing, loud enough to hear, and
echo      leave it playing the whole time. Wear headphones if you have
echo      them (plug them in only when the screen tells you to).
echo   2. Press a key here. A 5 second countdown starts, then it records.
echo   3. When this window says SWITCH NOW (about 20 seconds in), send the
echo      sound to your OTHER output. Either plug headphones in or out, or
echo      click the speaker icon next to the clock and pick another output.
echo   4. When it says SWITCH BACK (about 45 seconds in), switch again.
echo   5. Leave everything alone until it says it has finished.
echo.
echo PRIVACY: this records your screen, your voice and your computer's
echo sound. It is saved only on this computer, in the folder
echo switch-test-output next to this file. Nothing is sent anywhere.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "switch-test-log.txt" del "switch-test-log.txt"
call "%~dp0cargo-run.bat" "switch-test-log.txt" build --release -p openstage-capture-cli
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
echo The sound output is left to Windows on purpose, so the recorder can
echo follow it. Before you go on, make sure the output you are using now
echo is the one Windows has chosen (the speaker icon next to the clock).
echo.
echo Start your music now. Then press any key to begin.
pause >nul

if exist "%~dp0switch-test-output" rmdir /s /q "%~dp0switch-test-output"
start "" /b powershell -NoProfile -Command "Start-Sleep -Seconds 25; Write-Host ''; Write-Host '>>> SWITCH NOW: send the sound to your other output <<<'; try { [console]::Beep(880,400) } catch {}; Start-Sleep -Seconds 25; Write-Host ''; Write-Host '>>> SWITCH BACK: send the sound to the first output again <<<'; try { [console]::Beep(660,400) } catch {}"
echo Recording. The switch messages appear here in a moment.
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0switch-test-output\lesson.capture" --source screen --microphone "%MIC%" --output default --countdown-ms 5000 --plain --demo-script "stop@75000" > switch-test-log.txt 2>&1
set RESULT=%errorlevel%

echo.
echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type switch-test-log.txt
echo.

findstr /c:"and the recording followed it" switch-test-log.txt >nul
set FOLLOWED=%errorlevel%

if "%FOLLOWED%"=="0" goto followed
echo ------------------------------------------------------------
echo   The recorder did not notice any move.
echo ------------------------------------------------------------
echo.
echo Either the sound was not switched, or Windows did not tell the recorder
echo that it moved. Please send me the file switch-test-log.txt from this
echo folder, and tell me exactly what you did at the two messages (plugged
echo in headphones, or picked another output in the menu).
goto done

:followed
echo ------------------------------------------------------------
echo   The recorder noticed the sound move and followed it.
echo ------------------------------------------------------------
echo.
echo That is only half of the test. Now LISTEN. Opening the video: the
echo music should carry on after each switch, with at most a short gap
echo right after it. If you hear silence from the switch to the end, tell
echo me.
set VIDEO=%~dp0switch-test-output\lesson.capture\takes\take-001\screen.mp4
if exist "%VIDEO%" start "" "%VIDEO%"
echo.
echo Please send me the lines above, and what you heard.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file switch-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. Fix:
echo extract the ZIP into a NEW, empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'switch-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
