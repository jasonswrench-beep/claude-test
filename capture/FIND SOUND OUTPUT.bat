@echo off
setlocal
chcp 65001 >nul
title OpenStage Capture - find the sound output
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: where is the sound coming out?
echo ============================================================
echo.
echo A computer can have many sound outputs: speakers, headphones, a
echo monitor with speakers, and virtual ones made by other programs.
echo Windows lets the recorder listen to ONE of them at a time, so it has
echo to know which one you are actually listening through.
echo.
echo This listens to ALL of them at once while something plays, and tells
echo you which one carries the sound, and its number. Type that number when
echo TEST AUDIO CAPTURE.bat asks which sound output to use.
echo.
echo PRIVACY: nothing is saved and nothing is sent anywhere. It only
echo measures how loud each output is.
echo.

call "%~dp0rust-setup.bat"
if errorlevel 1 goto no_rust

echo Getting the program ready. The first time this can take a few minutes.
if exist "find-output-log.txt" del "find-output-log.txt"
call "%~dp0cargo-run.bat" "find-output-log.txt" build --release -p openstage-capture-cli
if errorlevel 1 goto build_failed

echo.
echo WHAT TO DO: start something playing on your computer that you can
echo HEAR, for example a video or some music, and keep it playing until
echo this finishes. Listen through the speakers or headphones you normally use.
echo.
echo It starts in 8 seconds.
timeout /t 8 /nobreak >nul
echo Listening to every sound output now...
echo.

"%~dp0target\release\openstage-capture.exe" find-output --plain --seconds 6 > find-output-log.txt 2>&1
set RESULT=%errorlevel%

echo ------------------------------------------------------------
echo   What the program found
echo ------------------------------------------------------------
type find-output-log.txt
echo.

if "%RESULT%"=="0" goto passed
echo ------------------------------------------------------------
echo   No sound was found. See the lines above.
echo ------------------------------------------------------------
echo.
echo Check that something was really playing and that you could hear it.
echo If you could hear it and this still found nothing, please send me the
echo file find-output-log.txt from this folder, or copy the lines above.
goto done

:passed
echo ------------------------------------------------------------
echo   Found it. Use the output number given above.
echo ------------------------------------------------------------
goto done

:no_rust
echo Rust is not installed yet. Please double-click BUILD CAPTURE.bat
echo first. It explains how to install Rust, which is free.
goto done

:build_failed
echo.
echo The program could not be built. The last lines of the log follow.
echo Please send me the file find-output-log.txt from this folder.
echo.
echo HINT: if the error says something was not found, or that a variant is
echo missing, this folder is probably a MIX of old and new files. That can
echo happen when a new ZIP is extracted over an old folder and some files
echo are skipped. Fix: extract the ZIP into a NEW, empty folder and run the
echo script from there.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'find-output-log.txt' -Tail 30"

:done
echo.
pause
endlocal
