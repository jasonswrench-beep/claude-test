@echo off
rem ------------------------------------------------------------------
rem cargo-run.bat: runs cargo and writes everything it says to a log file.
rem
rem The other scripts call it like this:
rem     call "%~dp0cargo-run.bat" the-log.txt build --release -p openstage-capture-cli
rem The first word is the log file, the rest is what cargo is asked to do.
rem The exit code is 0 when it worked, 1 when it did not, and 2 when the
rem libraries were missing and you chose not to download them.
rem
rem It first tries WITHOUT downloading anything. A computer that already
rem has the libraries this program is built from (Rust keeps them in
rem .cargo\registry in your user folder) never downloads. If some are
rem missing, it says so and asks before downloading them, once, from
rem crates.io, the Rust libraries website.
rem ------------------------------------------------------------------
set "CR_LOG=%~1"
set "CR_ARGS="
shift
:cr_collect
if "%~1"=="" goto cr_run
set "CR_ARGS=%CR_ARGS% %~1"
shift
goto cr_collect

:cr_run
cargo%CR_ARGS% --offline >> "%CR_LOG%" 2>&1
if not errorlevel 1 exit /b 0
findstr /c:"--offline" /c:"attempting to make an HTTP request" /c:"failed to download" /c:"failed to get" "%CR_LOG%" >nul 2>nul
if errorlevel 1 exit /b 1

echo.
echo Some of the libraries this program is built from are not on this
echo computer yet. To get them, the program must download them once
echo from crates.io, the website that holds Rust libraries. Nothing of
echo yours is sent. After this, builds on this computer do not download.
echo.
set "CR_ALLOW="
set /p CR_ALLOW=Type D and press Enter to allow the download, or just press Enter to stop: 
if /i not "%CR_ALLOW%"=="D" goto cr_declined
echo.>> "%CR_LOG%"
echo --- trying again with downloading allowed --- >> "%CR_LOG%"
cargo%CR_ARGS% >> "%CR_LOG%" 2>&1
exit /b %errorlevel%

:cr_declined
echo.
echo Stopped, and nothing was downloaded. If this computer must not
echo download anything, tell me, and I will find another way.
exit /b 2
