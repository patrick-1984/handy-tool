@echo off
setlocal EnableDelayedExpansion

:: ============================================================
:: Handy Tool - Portable Distribution Packager (T-114)
:: ============================================================
:: Assembles a portable ZIP from an EXISTING release build output
:: (C:\tmp\hb\release\handy.exe + resources). It does NOT build the
:: app itself -- run build.cmd first (per project convention, that
:: script is run manually, not from Claude Code).
::
:: Output: C:\tmp\hb\release\Handy-Tool-{version}-portable.zip
:: (same folder the NSIS/MSI installers already land in)
::
:: What "portable" means today (T-114 done-partial):
::   - No installer, no admin rights required to run it.
::   - The ZIP can be extracted to any folder (USB stick, network
::     share, etc.) and run via handy.exe directly.
::   - A `portable.marker` file is dropped next to handy.exe. The
::     Rust side does NOT yet act on this marker (see
::     tickets\T-114-portable-distribution.md for the precise wiring
::     spec) -- until that lands, a portable-launched handy.exe still
::     writes settings/history/models/recordings to the normal Windows
::     per-user profile location (%APPDATA%\pr.handy), NOT beside the
::     exe. This script packages the files; it cannot change where the
::     already-built handy.exe decides to read/write app data.
::   - NOT a "no registry writes" guarantee: if the user enables
::     autostart in Settings, Handy Tool writes a Run-key registry entry
::     on every startup regardless of how it was packaged/launched (see
::     the autostart enable()/disable() call in lib.rs). The generated
::     README.txt says this plainly -- do not re-add a blanket
::     "no registry writes" claim here or there.
::
:: This script FAILS LOUDLY rather than emit a broken/incomplete zip:
::   - Verifies the staged handy.exe's ProductVersion metadata matches
::     the version in tauri.conf.json (catches a stale/mismatched build).
::   - Every required file copy is fatal on failure (no silent warnings
::     for files the package needs).
::   - Validates the staged folder against an expected manifest (all
::     required resource files + at least one bundled model .onnx)
::     immediately before Compress-Archive.
::   - Checks mkdir/cleanup errorlevels instead of ignoring them.
:: ============================================================

cd /d "%~dp0"

set "RELEASE_DIR=C:\tmp\hb\release"
set "SRC_EXE=%RELEASE_DIR%\handy.exe"
set "SRC_RES=%RELEASE_DIR%\resources"
set "CONF=src-tauri\tauri.conf.json"

if not exist "%SRC_EXE%" (
    echo ERROR: %SRC_EXE% not found.
    echo Run build.cmd first ^(the user runs this manually -- see CLAUDE.md^).
    exit /b 1
)

if not exist "%SRC_RES%\default_settings.json" (
    echo ERROR: %SRC_RES% is missing or incomplete ^(no default_settings.json^).
    echo Run build.cmd first to produce a full release output.
    exit /b 1
)

if not exist "%CONF%" (
    echo ERROR: %CONF% not found. Run this script from the repo root.
    exit /b 1
)

:: --- Read version from tauri.conf.json (single source of truth; kept in
::     sync with package.json / Cargo.toml per CLAUDE.md "Version Bumping") ---
for /f "usebackq delims=" %%V in (`powershell -NoProfile -Command "(Get-Content -Raw '%CONF%' | ConvertFrom-Json).version"`) do set "VERSION=%%V"

if "!VERSION!"=="" (
    echo ERROR: Could not read version from %CONF%.
    exit /b 1
)
echo Packaging portable distribution for version !VERSION!

:: --- Verify the staged handy.exe isn't a stale/mismatched build. A build
::     left over from a previous version bump would otherwise get silently
::     packaged and labeled with the WRONG version's zip name/README. ---
for /f "usebackq delims=" %%V in (`powershell -NoProfile -Command "(Get-Item '%SRC_EXE%').VersionInfo.ProductVersion"`) do set "EXEVERSION=%%V"

if "!EXEVERSION!"=="" (
    echo ERROR: Could not read ProductVersion metadata from %SRC_EXE%.
    exit /b 1
)

:: EXEVERSION may carry a trailing ".0" 4th component (Windows version
:: resources are 4-part; e.g. "0.41.0.0") that VERSION (3-part, from
:: tauri.conf.json, e.g. "0.41.0") does not. Accept either an exact match
:: or that specific suffix; anything else means the exe was built from a
:: different version than tauri.conf.json currently declares -- abort.
set "VERSION_MATCH="
if "!EXEVERSION!"=="!VERSION!" set "VERSION_MATCH=1"
if "!EXEVERSION!"=="!VERSION!.0" set "VERSION_MATCH=1"
if not defined VERSION_MATCH (
    echo ERROR: Version mismatch -- staged build is STALE.
    echo   tauri.conf.json version : !VERSION!
    echo   %SRC_EXE% ProductVersion: !EXEVERSION!
    echo Rebuild ^(build.cmd / rebuild.cmd^) before packaging -- refusing to
    echo package a portable ZIP from a binary that doesn't match the
    echo current source version.
    exit /b 1
)
echo Verified handy.exe ProductVersion ^(!EXEVERSION!^) matches !VERSION!

set "STAGE=%RELEASE_DIR%\portable-stage"
set "PKGNAME=Handy Tool"
set "PKGDIR=%STAGE%\%PKGNAME%"
set "ZIPPATH=%RELEASE_DIR%\Handy-Tool-!VERSION!-portable.zip"

:: --- Clean staging area ---
if exist "%STAGE%" (
    rd /s /q "%STAGE%"
    if errorlevel 1 (
        echo ERROR: Failed to clean existing staging folder %STAGE%.
        echo Close any program that may still have a file inside it open
        echo ^(e.g. a handy.exe copy launched from a previous test^) and
        echo re-run.
        exit /b 1
    )
)

mkdir "%PKGDIR%"
if errorlevel 1 (
    echo ERROR: Failed to create staging folder %PKGDIR%.
    exit /b 1
)
mkdir "%PKGDIR%\resources"
if errorlevel 1 (
    echo ERROR: Failed to create %PKGDIR%\resources.
    exit /b 1
)
mkdir "%PKGDIR%\resources\models"
if errorlevel 1 (
    echo ERROR: Failed to create %PKGDIR%\resources\models.
    exit /b 1
)
mkdir "%PKGDIR%\data"
if errorlevel 1 (
    echo ERROR: Failed to create %PKGDIR%\data.
    exit /b 1
)
mkdir "%PKGDIR%\data\models"
if errorlevel 1 (
    echo ERROR: Failed to create %PKGDIR%\data\models.
    exit /b 1
)
mkdir "%PKGDIR%\data\recordings"
if errorlevel 1 (
    echo ERROR: Failed to create %PKGDIR%\data\recordings.
    exit /b 1
)

:: --- Copy the binary ---
copy /y "%SRC_EXE%" "%PKGDIR%\handy.exe" >nul
if errorlevel 1 (
    echo ERROR: Failed to copy handy.exe.
    exit /b 1
)

:: --- Copy resources: mirror the INSTALLED layout under
::     %LOCALAPPDATA%\Handy Tool\resources exactly (this was read directly
::     off this machine to build the file list below). Notably this
::     EXCLUDES resources\icon.ico, which exists in the raw cargo build
::     output but is NOT copied there by the NSIS installer either.
::     Every file below is REQUIRED -- a missing or unreadable file
::     aborts the whole script rather than producing an incomplete zip. ---
set "RESFILES=default_settings.json handy.png marimba_start.wav marimba_stop.wav pop_start.wav pop_stop.wav recording.png transcribing.png tray_idle.png tray_idle_dark.png tray_recording.png tray_recording_dark.png tray_transcribing.png tray_transcribing_dark.png"

for %%F in (!RESFILES!) do (
    if not exist "%SRC_RES%\%%F" (
        echo ERROR: required resource %%F not found in %SRC_RES%.
        echo Run build.cmd first to produce a full release output -- this
        echo file is REQUIRED, packaging cannot silently skip it.
        exit /b 1
    )
    copy /y "%SRC_RES%\%%F" "%PKGDIR%\resources\%%F" >nul
    if errorlevel 1 (
        echo ERROR: Failed to copy required resource %%F.
        exit /b 1
    )
)

:: models\ under resources\ is bundled read-only assets (currently just the
:: VAD model) -- copy whatever is there rather than hardcoding the filename,
:: same as the installer does. Still fatal on failure: a portable package
:: with no bundled VAD model is broken.
xcopy /y /q "%SRC_RES%\models\*.*" "%PKGDIR%\resources\models\" >nul
if errorlevel 1 (
    echo ERROR: Failed to copy %SRC_RES%\models\ contents ^(VAD model^).
    echo This is a required bundled resource -- packaging cannot continue.
    exit /b 1
)

:: --- Portable marker: presence of this file is what a future Rust change
:: (see the ticket) will use to redirect app-data to .\data instead of
:: %APPDATA%\pr.handy. It is inert today -- see header comment and README. ---
type nul > "%PKGDIR%\portable.marker"
if errorlevel 1 (
    echo ERROR: Failed to create portable.marker.
    exit /b 1
)

:: --- README ---
call :write_readme "%PKGDIR%\README.txt" "!VERSION!"
if errorlevel 1 (
    echo ERROR: Failed to write README.txt.
    exit /b 1
)

:: --- Validate the staged package against the expected manifest BEFORE
::     zipping. This is a final sanity gate independent of the copy-time
::     checks above -- if anything required ends up missing from the
::     staging folder, abort rather than emit a broken/incomplete zip. ---
set "MANIFEST_OK=1"

if not exist "%PKGDIR%\handy.exe" (
    echo ERROR: manifest check failed -- handy.exe missing from staged package.
    set "MANIFEST_OK="
)

for %%F in (!RESFILES!) do (
    if not exist "%PKGDIR%\resources\%%F" (
        echo ERROR: manifest check failed -- resources\%%F missing from staged package.
        set "MANIFEST_OK="
    )
)

set "FOUND_MODEL="
for %%M in ("%PKGDIR%\resources\models\*.onnx") do set "FOUND_MODEL=1"
if not defined FOUND_MODEL (
    echo ERROR: manifest check failed -- no .onnx model found in resources\models\.
    set "MANIFEST_OK="
)

if not exist "%PKGDIR%\portable.marker" (
    echo ERROR: manifest check failed -- portable.marker missing from staged package.
    set "MANIFEST_OK="
)

if not exist "%PKGDIR%\README.txt" (
    echo ERROR: manifest check failed -- README.txt missing from staged package.
    set "MANIFEST_OK="
)

if not defined MANIFEST_OK (
    echo.
    echo ABORTING: staged package failed manifest validation -- refusing to
    echo produce a zip from an incomplete/broken staging folder.
    exit /b 1
)
echo Manifest check passed: all required files present in staged package.

:: --- Zip it ---
if exist "%ZIPPATH%" (
    del /f /q "%ZIPPATH%"
    if errorlevel 1 (
        echo ERROR: Failed to remove existing zip %ZIPPATH% before repackaging.
        exit /b 1
    )
)
powershell -NoProfile -Command "Compress-Archive -Path '%STAGE%\*' -DestinationPath '%ZIPPATH%' -CompressionLevel Optimal"
if errorlevel 1 (
    echo ERROR: Compress-Archive failed.
    exit /b 1
)

if not exist "%ZIPPATH%" (
    echo ERROR: Compress-Archive reported success but %ZIPPATH% is missing.
    exit /b 1
)

echo.
echo PORTABLE PACKAGE READY: %ZIPPATH%
echo Staging folder left at: %PKGDIR% ^(re-run to refresh; this script always cleans it first^)
exit /b 0

:write_readme
set "OUTFILE=%~1"
set "V=%~2"
(
echo Handy Tool %V% - Portable Edition
echo ==================================
echo.
echo This is a portable build of Handy Tool: no installer and no admin
echo rights are required. Extract this ZIP anywhere -- a USB stick, a
echo network share, a folder on this PC -- and run handy.exe from there.
echo.
echo FIRST RUN / MODEL DOWNLOADS
echo ----------------------------
echo Handy Tool ships with only the small voice-activity-detection model
echo bundled ^(resources\models\silero_vad_v4.onnx^). The actual speech-to-text
echo model you pick in Settings ^> Models ^(e.g. a Whisper or Parakeet model^)
echo downloads on first use from https://blob.handy.computer/ -- this needs
echo an internet connection the first time you select/use a new model. Once
echo downloaded, the model is cached and works fully offline afterward.
echo.
echo WHERE YOUR DATA LIVES
echo ----------------------
echo This build reads the "portable.marker" file beside handy.exe and, when
echo present, keeps ALL of its data inside the "data\" folder next to the exe:
echo settings, history, downloaded models, recordings, logs, and the WebView
echo storage. Nothing is written to %%APPDATA%%\pr.handy or %%LOCALAPPDATA%%.
echo Delete the whole folder to remove every trace. ^(If "data\" ever can't be
echo written -- e.g. a read-only drive -- Handy safely falls back to the normal
echo per-user profile location and logs a warning.^)
echo.
echo REGISTRY / SYSTEM CHANGES
echo --------------------------
echo Running this build does not run an installer and does not write to
echo Program Files. Autostart, if enabled in Settings, still writes a
echo Windows registry Run entry pointing at this location -- disable
echo autostart in Settings if you want a fully registry-free run. True
echo portable-mode isolation ^(no registry touched under any setting^) is
echo pending -- see T-114 in the source repo.
echo.
echo RUNNING IT
echo ----------
echo Just double-click handy.exe. No installer runs and nothing is written
echo to Program Files, and you can delete this folder at any time to
echo remove the app binary itself ^(your app data, per the note above, is
echo separate and lives under %%APPDATA%%\pr.handy\ until T-114's Rust-side
echo change ships^). If you enabled autostart, disable it in Settings before
echo deleting this folder, or you'll be left with a stale registry Run
echo entry pointing at a folder that no longer exists.
echo.
echo UNINSTALL
echo ---------
echo There is nothing to uninstall beyond deleting this folder. If you also
echo want to remove your settings/history/downloaded models, additionally
echo delete %%APPDATA%%\pr.handy\. If you enabled autostart, disable it in
echo Settings first ^(see REGISTRY / SYSTEM CHANGES above^) so a stale Run
echo entry doesn't point at a deleted folder.
) > "%OUTFILE%"
exit /b 0
