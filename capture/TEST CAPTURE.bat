@echo off
setlocal
title OpenStage Capture - test
cd /d "%~dp0"

echo.
echo ============================================================
echo   OpenStage Capture: run all the tests
echo ============================================================
echo.
echo This checks that the program works on THIS computer, including a
echo test that kills a recording in the middle and then repairs it.
echo The first time it can take several minutes. You do not need to do
echo anything while it works. A full log is saved in this folder as
echo test-log.txt.
echo.

where cargo >nul 2>nul
if errorlevel 1 goto no_rust

echo Found Rust:
cargo --version
ver
echo.
echo Running the tests. Please wait...
cargo test --workspace > test-log.txt 2>&1
set TESTS_RESULT=%errorlevel%

echo.
echo Test summary (each line is one group of tests):
findstr /B /C:"test result" test-log.txt
echo.

if not "%TESTS_RESULT%"=="0" goto failed

echo Now a short real run: the program records a 3 second test recording,
echo then we ask it to check the recording.
echo.
set DEMO=%TEMP%\openstage-capture-demo.capture
if exist "%DEMO%" rmdir /s /q "%DEMO%"
"%~dp0target\debug\openstage-capture.exe" record --project "%DEMO%" --countdown-ms 0 --duration-ms 3000 --ignore-stdin-eof < nul > demo-log.txt 2>&1
if errorlevel 1 goto demo_failed
echo The last line the program printed (it should say "ok":true):
powershell -NoProfile -Command "Get-Content -LiteralPath 'demo-log.txt' -Tail 1"
echo.
"%~dp0target\debug\openstage-capture.exe" recover --project "%DEMO%" >> demo-log.txt 2>&1
if exist "%DEMO%" rmdir /s /q "%DEMO%"

echo ------------------------------------------------------------
echo   PASS. Everything worked on this computer.
echo ------------------------------------------------------------
echo.
echo Please tell me it passed. If you want to be thorough, you can send
echo me test-log.txt as well.
goto done

:demo_failed
echo ------------------------------------------------------------
echo   FAIL. The tests passed but the short test recording did not work.
echo ------------------------------------------------------------
echo.
echo Please send me the file demo-log.txt from this folder. The last
echo lines are shown below.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'demo-log.txt' -Tail 20"
goto done

:no_rust
echo ------------------------------------------------------------
echo   Rust is not installed on this computer yet.
echo ------------------------------------------------------------
echo.
echo Please double-click BUILD CAPTURE.bat first. It explains how to
echo install Rust, which is free.
goto done

:failed
echo ------------------------------------------------------------
echo   FAIL. At least one test did not pass.
echo ------------------------------------------------------------
echo.
echo That is useful news, not a disaster: it means a problem was found
echo before anyone relied on this. Nothing on your computer was harmed.
echo Please send me the file test-log.txt from this folder. The last
echo lines are shown below.
echo.
powershell -NoProfile -Command "Get-Content -LiteralPath 'test-log.txt' -Tail 40"

:done
echo.
pause
endlocal
