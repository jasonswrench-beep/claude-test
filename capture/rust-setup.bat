@echo off
rem ------------------------------------------------------------------
rem rust-setup.bat: makes sure the program called cargo can be found.
rem
rem The other scripts in this folder call this first, like this:
rem     call "%~dp0rust-setup.bat"
rem     if errorlevel 1 goto no_rust
rem It changes nothing permanent: the PATH it sets lasts only until the
rem script that called it ends. It installs nothing and needs no
rem administrator rights.
rem
rem 1. If cargo is already on the PATH, that is all.
rem 2. Some computers have Rust installed but deliberately not on the
rem    PATH, with a helper that sets things up when asked. If this file
rem    exists, it is used:  C:\Dev\tools\rust-env.ps1
rem    (to use a helper somewhere else, set the variable
rem    OPENSTAGE_RUST_ENV to its full path first)
rem 3. Otherwise, a normal Rust install in your user folder
rem    (.cargo\bin) is tried.
rem ------------------------------------------------------------------
where cargo >nul 2>nul
if not errorlevel 1 exit /b 0

set "RS_HELPER=%OPENSTAGE_RUST_ENV%"
if "%RS_HELPER%"=="" set "RS_HELPER=C:\Dev\tools\rust-env.ps1"
if not exist "%RS_HELPER%" goto rs_cargo_home

echo Rust is not on the PATH of this computer, so it is being loaded with
echo %RS_HELPER%
echo This lasts only while this window is open. Nothing is installed.
set "RS_BAT=%TEMP%\openstage-rust-env.bat"
set "RS_ERR=%TEMP%\openstage-rust-env-error.txt"
if exist "%RS_BAT%" del "%RS_BAT%"
if exist "%RS_ERR%" del "%RS_ERR%"
rem The helper is run in PowerShell, and the settings it makes (PATH, INCLUDE, LIB and a few more) are
rem written out as set lines, which are then run here. Anything the helper says is thrown away.
powershell -NoProfile -ExecutionPolicy Bypass -Command ". '%RS_HELPER%' *> $null; 'PATH','INCLUDE','LIB','LIBPATH','CARGO_HOME','RUSTUP_HOME','OPENSTAGE_PYTHON' | ForEach-Object { $v = [Environment]::GetEnvironmentVariable($_,'Process'); if ($v) { 'set ' + [char]34 + $_ + '=' + $v.Replace([string][char]37, ([string][char]37 + [string][char]37)) + [char]34 } }" > "%RS_BAT%" 2> "%RS_ERR%"
if exist "%RS_BAT%" call "%RS_BAT%"
if exist "%RS_BAT%" del "%RS_BAT%"
where cargo >nul 2>nul
if not errorlevel 1 exit /b 0

echo.
echo The helper %RS_HELPER% was found, but cargo still could not be found
echo after running it. What PowerShell said:
if exist "%RS_ERR%" type "%RS_ERR%"
echo.

:rs_cargo_home
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul
if not errorlevel 1 exit /b 0
exit /b 1
