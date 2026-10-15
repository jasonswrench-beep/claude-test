@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - long recording test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: a long recording
echo ============================================================
echo.
echo Short tests can pass while a long recording quietly goes wrong: the
echo program grows, falls behind, or sound drifts away from the picture a
echo little more each minute. This records for as long as you choose and
echo then reports on exactly those things.
echo.
echo While it runs, use your computer as you normally would. Open
echo programs, type, scroll, play some sound now and then, and talk once
echo in a while. In the middle it PAUSES for 20 seconds and RESUMES, to be
echo sure that works over a long recording too. You will hear the
echo recorder's short sounds for the countdown, start, pause, resume and
echo stop; they are not recorded.
echo.
echo BEFORE YOU START:
echo   - Make sure Windows will not go to sleep or turn the screen off
echo     for the whole time (Settings, System, Power).
echo   - Leave about 6 gigabytes of free disk space for a 60 minute test.
echo     It is a real recording of your screen and voice.
echo.
echo PRIVACY: the recording shows your screen and holds your voice. It is
echo saved only on this computer, in the folder long-test-output next to
echo this file. Nothing is sent anywhere. Delete the folder afterwards.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "long-test-log.txt" del "long-test-log.txt"
call "%~dp0cargo-run.bat" "long-test-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo HOW MANY MINUTES? Type a number and press Enter. Press Enter alone for
echo 10, a good first try. The full test is 60.
set "MINUTES="
set /p MINUTES=Minutes: 
if "%MINUTES%"=="" set MINUTES=10
set /a MINUTES_N=%MINUTES%+0
if %MINUTES_N% LEQ 0 goto bad_minutes
if %MINUTES_N% GTR 240 goto bad_minutes

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
echo WHICH SOUND OUTPUT? Type its number and press Enter, or just press
echo Enter for the default. To record no computer sound, type none.
set "OUT="
set /p OUT=Sound output number: 

echo.
echo WILL YOU BE AWAY? Type Y and press Enter to leave the computer alone
echo for the whole test. The script then keeps Windows awake, plays a
echo gentle looping sound through your default sound output, and shows a
echo window with things moving in it, so the recording has real picture
echo and sound without you. For that sound to be recorded, answer the
echo sound output question above with just Enter. Press Enter alone if
echo you will use the computer yourself, as before.
set "AWAY="
set /p AWAY=Away, Y or Enter: 

set /a LENGTH_MS=%MINUTES_N%*60000
set /a PAUSE_AT=5000+%LENGTH_MS%/2
set /a RESUME_AT=%PAUSE_AT%+20000
set /a STOP_AT=5000+%LENGTH_MS%+20000
echo.
echo It will record for %MINUTES_N% minutes, after a 5 second countdown.
echo Closing this window stops it early.
echo.
if exist "%~dp0long-test-output" (
  echo An earlier long test is still in the folder long-test-output.
  echo Press any key to delete it and go on, or close this window to keep it.
  pause >nul
  rmdir /s /q "%~dp0long-test-output"
)
if /i not "%AWAY%"=="Y" goto begin_recording
del "%TEMP%\openstage-helper-ready.txt" >nul 2>nul
del "%~dp0hands-free-log.txt" >nul 2>nul
set /a HELPER_MINUTES=%MINUTES_N%+3
start "" /min powershell -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "%~dp0tools\hands-free-helper.ps1" -Minutes %HELPER_MINUTES% -LogPath "%~dp0hands-free-log.txt"
echo Getting the test picture and sound ready. This can take up to a minute.
set /a WAITED=0
:wait_for_helper
if exist "%TEMP%\openstage-helper-ready.txt" goto begin_recording
if exist "%~dp0hands-free-log.txt" goto helper_failed
if %WAITED% GEQ 120 goto helper_failed
timeout /t 2 /nobreak >nul
set /a WAITED=%WAITED%+2
goto wait_for_helper

:begin_recording
echo It starts in 8 seconds.
timeout /t 8 /nobreak >nul
echo Recording now. The result appears here when it finishes.
echo.

"%~dp0target\release\openstage-capture.exe" record --project "%~dp0long-test-output\lesson.capture" --source screen --microphone "%MIC%" --output "%OUT%" --countdown-ms 5000 --plain --strict --demo-script "pause@%PAUSE_AT%;resume@%RESUME_AT%;stop@%STOP_AT%" > long-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type long-test-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   At least one thing did not pass. See the lines above.
echo ------------------------------------------------------------
echo.
echo That is useful, not a disaster. Please send me the file
echo long-test-log.txt from this folder (it is plain text and contains no
echo pictures or sound), or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder kept up for the whole recording.
echo ------------------------------------------------------------
echo.
echo Please send me the lines above anyway. The exact numbers matter.
goto done

:helper_failed
echo.
echo The test picture and sound could not be started, so nothing was
echo recorded. If a file hands-free-log.txt appeared in this folder, please
echo send it to me. You can run this script again and press Enter at the
echo question about being away, then use the computer yourself.
goto done

:bad_minutes
echo.
echo Please type a whole number of minutes from 1 to 240. Run the script
echo again to try once more.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file long-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'long-test-log.txt' -Tail 30"

:done
echo.
pause
endlocal
