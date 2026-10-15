@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - note what you do with the mouse and keyboard
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: note what you do (the interaction log)
echo ============================================================
echo.
echo This records about half a minute of your screen while the recorder
echo also keeps a PRIVATE LOG of what you do: where the cursor goes,
echo your clicks, the window in front, the name of what you click
echo (for example the button Save) and, if you allow it, the keys you
echo press. It is off in every other recording. You are only asked to
echo try it here.
echo.
echo PRIVACY: the log is saved only on this computer, in the folder
echo interactions-test-output next to this file. Nothing is sent
echo anywhere. It is never put in a published course. At the end you
echo can read it in words and delete it. Do not type anything private
echo during this test: keys are written to the log, except in password
echo boxes, and the log is shown on this screen at the end.
echo.
echo You will hear four soft notes when the countdown starts. They mean
echo what you do is about to be noted. They are not in the recording.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
cargo build --release -p openstage-capture-cli > interactions-test-log.txt 2>&1
if errorlevel 1 goto build_failed

echo.
echo SHOULD KEYS BE NOTED TOO?
echo   Press Enter alone to note the keys as well, or type N and press
echo   Enter to note everything except the keys.
set "KEYS="
set /p KEYS=Your choice: 
set "LEVEL=on"
if /i "%KEYS%"=="N" set "LEVEL=no-keys"
echo.
echo WHAT TO DO after the countdown, in this order, taking your time:
echo   1. Move the mouse around the screen for a few seconds.
echo   2. Click a few things, for example the Start button, then press
echo      Escape. A click on a named button is noted by name.
echo   3. Open Notepad. Type the words  hello world  and press Enter.
echo      Press Ctrl and S together, then Escape.
echo   4. If you can, open a web page that has a PASSWORD box, click in
echo      it and type three letters. Those letters must NOT be noted.
echo      Do not type a real password.
echo   5. The recorder pauses by itself at 20 seconds for 3 seconds.
echo      Click anything a few times during the pause. Those clicks
echo      must NOT be noted: time spent paused is not recorded.
echo   6. It resumes by itself and stops a few seconds later.
echo It starts in 8 seconds, with a countdown.
timeout /t 8 /nobreak >nul
echo.

if exist "%~dp0interactions-test-output" rmdir /s /q "%~dp0interactions-test-output"
"%~dp0target\release\openstage-capture.exe" record --project "%~dp0interactions-test-output\lesson.capture" --source screen --interactions %LEVEL% --microphone none --output none --countdown-ms 3000 --plain --demo-script "pause@23000;resume@26000;stop@33000" > interactions-test-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the recorder said
echo ------------------------------------------------------------
type interactions-test-log.txt
echo.

if not exist "%~dp0interactions-test-output\lesson.capture\capture.json" goto no_project

echo ------------------------------------------------------------
echo   What was noted, in words, line by line
echo ------------------------------------------------------------
"%~dp0target\release\openstage-capture.exe" interactions --plain --list --project "%~dp0interactions-test-output\lesson.capture" > interactions-test-list.txt 2>&1
type interactions-test-list.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   Something did not work. See the lines above.
echo ------------------------------------------------------------
echo.
echo If a line above says Windows would not allow the program to watch
echo the mouse or keyboard, it says what to do. Please send me the
echo files interactions-test-log.txt and interactions-test-list.txt
echo from this folder if it is not clear.
goto done

:passed
echo ------------------------------------------------------------
echo   PASS. The recorder noted what you did and the log was read back.
echo ------------------------------------------------------------
echo.
echo Please send me everything above, and tell me:
echo   - Did you hear the four soft notes as the countdown began?
echo   - Do the lines above match what you did? Look for your clicks,
echo     the names of the things you clicked, the window Notepad, and
echo     your keys (unless you chose N).
echo   - Are the three letters typed in the password box missing? They
echo     must be. If you could not try this, say so.
echo   - Are the clicks you made while paused missing? They must be.
echo   - Did the recording itself feel normal, with no stutter?
echo   - Was the line Interaction recording is on said when recording
echo     began, and is it in the lines above?
echo.
echo The recording and its log are in the folder
echo interactions-test-output\lesson.capture\takes\take-001

echo.
echo DELETE THE LOG? The log holds the title of every window you used
echo and the names of what you clicked. Type D and press Enter to
echo delete it now, which also tests the deleting. Anything else keeps it.
set "DEL="
set /p DEL=Your choice: 
if /i not "%DEL%"=="D" goto done
"%~dp0target\release\openstage-capture.exe" interactions --plain --delete --yes --project "%~dp0interactions-test-output\lesson.capture"
echo.
echo Reading it back to check that it is really gone:
"%~dp0target\release\openstage-capture.exe" interactions --plain --list --project "%~dp0interactions-test-output\lesson.capture"
if exist interactions-test-list.txt del interactions-test-list.txt
goto done

:no_project
echo No recording was made, so there is nothing to read back. The lines
echo above say why.
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file interactions-test-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, this folder is
echo probably a MIX of old and new files. Fix: extract the ZIP into a NEW,
echo empty folder and run the script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'interactions-test-log.txt' -Tail 30"

:done
echo.
pause
