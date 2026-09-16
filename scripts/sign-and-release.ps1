<#
.SYNOPSIS
  Download CI build artifacts, sign them with the LOCAL updater key, and publish a
  GitHub release.

.DESCRIPTION
  The second half of the release pipeline. CI ("All Platforms Build") compiles every
  target on GitHub runners and uploads unsigned artifacts; this script runs on the
  maintainer's machine, where the updater private key lives, and does the signing and
  publishing.

  Why the split: signing in CI would mean uploading the private key to GitHub Actions
  secrets. Keeping it local costs one command per release and keeps custody on one
  machine. Building in CI removes the development machine from the compile path, which
  matters because Smart App Control there refuses freshly built unsigned binaries.

  What it guarantees:
    * Every installer's filename version matches tauri.conf.json, checked as an exact
      token. A stale installer published under a new name would carry a VALID
      signature - Tauri signs bytes, not names - and every client would "update" to an
      older build, then be offered the same update forever.
    * The key is read into the process environment and cleared in a finally block, so
      it is never written to a log, an argument list, or the shell history.
    * Only Windows gets updater metadata. macOS and Linux ship as direct downloads by
      design; their updater bundles would have to be signed at BUILD time, which
      cannot happen while the key stays here.

.PARAMETER RunId
  The "All Platforms Build" run to publish. Defaults to the most recent successful one.

.PARAMETER DryRun
  Download, verify and sign, but do not create or modify a GitHub release.

.EXAMPLE
  ./scripts/sign-and-release.ps1 -RunId 35072044998 -NotesFile notes.md
#>
[CmdletBinding()]
param(
    [string] $RunId,
    [string] $NotesFile,
    [string] $Repo = "patrick-1984/handy-tool",
    [switch] $DryRun
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$gh = "C:\Program Files\GitHub CLI\gh.exe"
if (-not (Test-Path $gh)) { $gh = "gh" }

# ---------------------------------------------------------------- version ----
$conf = Get-Content (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version
if (-not $version) { throw "Could not read version from tauri.conf.json" }
Write-Host "Publishing version $version" -ForegroundColor Cyan

# ------------------------------------------------------------------- run -----
if (-not $RunId) {
    $RunId = & $gh run list --repo $Repo --workflow "All Platforms Build" `
        --status success --limit 1 --json databaseId --jq ".[0].databaseId"
    if (-not $RunId) { throw "No successful 'All Platforms Build' run found. Dispatch one first." }
    Write-Host "Using most recent successful run: $RunId"
}

$stage = Join-Path $env:TEMP "handy-release-$version"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null

Write-Host "Downloading artifacts from run $RunId ..."
& $gh run download $RunId --repo $Repo --dir $stage
if ($LASTEXITCODE -ne 0) { throw "Artifact download failed" }

# Installers only. The runtime-verification sidecars stay behind; they are a CI
# attestation, not something users download.
$installers = Get-ChildItem $stage -Recurse -File |
    Where-Object { $_.Extension -in ".exe", ".dmg", ".deb", ".rpm", ".AppImage" }
if (-not $installers) { throw "No installers found in the downloaded artifacts" }

Write-Host "`nFound $($installers.Count) installer(s):" -ForegroundColor Cyan
$installers | ForEach-Object { "  {0,-52} {1,12:N0} bytes" -f $_.Name, $_.Length }

# --------------------------------------------------------------- verify ------
# Exact token, not a substring: `includes("1.6.0")` also accepts "11.6.0".
$bad = $installers | Where-Object {
    $m = [regex]::Match($_.Name, '[_-](\d+\.\d+\.\d+)[_.-]')
    -not $m.Success -or $m.Groups[1].Value -ne $version
}
if ($bad) {
    $bad | ForEach-Object { Write-Host "  MISMATCH: $($_.Name)" -ForegroundColor Red }
    throw "Some installers do not carry version $version. Refusing to publish - a stale binary would ship under a valid signature."
}
Write-Host "All installers carry version $version" -ForegroundColor Green

# ----------------------------------------------------------------- sign ------
# Only Windows NSIS gets updater metadata; see the header for why.
$toSign = $installers | Where-Object { $_.Extension -eq ".exe" }

try {
    $keyPath = Join-Path $repoRoot ".keys\handy-updater.key"
    $pwPath = Join-Path $repoRoot ".keys\handy-updater.password"
    if (-not (Test-Path $keyPath)) { throw "Updater key not found at $keyPath" }
    # `tauri signer sign` reads DIFFERENT variable names than `tauri build` does:
    # TAURI_PRIVATE_KEY* versus TAURI_SIGNING_PRIVATE_KEY*. Setting only the build
    # names fails with "Unable to find the private key". Set both.
    #
    # Prefer the PATH form: the key's contents then never become a process argument
    # or a shell variable that something could echo.
    $env:TAURI_PRIVATE_KEY_PATH = $keyPath
    $env:TAURI_PRIVATE_KEY_PASSWORD = (Get-Content $pwPath -Raw).Trim()
    $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $env:TAURI_PRIVATE_KEY_PASSWORD

    Push-Location $repoRoot
    foreach ($f in $toSign) {
        Write-Host "Signing $($f.Name) ..."
        # NEVER pass the key or password as ARGUMENTS. `bun run` echoes the full
        # command line it executes, so an argument-passed secret is printed to the
        # console, to CI logs, and to any transcript capturing this session. The
        # signer reads TAURI_SIGNING_PRIVATE_KEY / _PASSWORD from the environment,
        # which is set above and cleared in the finally block.
        # Call the CLI directly rather than through `bun run`, which prints the
        # resolved command line it is about to execute.
        & bunx @tauri-apps/cli signer sign $f.FullName | Out-Null
        if (-not (Test-Path "$($f.FullName).sig")) { throw "Signing produced no .sig for $($f.Name)" }
        Write-Host "  -> $($f.Name).sig" -ForegroundColor Green
    }
    Pop-Location
}
finally {
    # Clear unconditionally, including on failure.
    $env:TAURI_SIGNING_PRIVATE_KEY = $null
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $null
    $env:TAURI_PRIVATE_KEY_PATH = $null
    $env:TAURI_PRIVATE_KEY_PASSWORD = $null
}

if ($DryRun) {
    Write-Host "`nDry run - staged at $stage, nothing published." -ForegroundColor Yellow
    return
}

# -------------------------------------------------------------- publish -----
$assets = @()
$assets += $installers | ForEach-Object { $_.FullName }
$assets += $toSign | ForEach-Object { "$($_.FullName).sig" }

$notesArg = if ($NotesFile -and (Test-Path $NotesFile)) { @("--notes-file", $NotesFile) }
            else { @("--generate-notes") }

# `gh release view` exits non-zero when the release does not exist, which is the
# normal first-publish path - so it must not trip $ErrorActionPreference.
$existing = $null
try { $existing = & $gh release view "v$version" --repo $Repo --json tagName 2>$null } catch { }
if ($LASTEXITCODE -ne 0) { $existing = $null }
if ($existing) {
    Write-Host "`nRelease v$version exists - uploading assets to it." -ForegroundColor Yellow
    & $gh release upload "v$version" @assets --repo $Repo --clobber
} else {
    Write-Host "`nCreating release v$version ..." -ForegroundColor Cyan
    & $gh release create "v$version" @assets --repo $Repo `
        --title "Handy Tool $version" @notesArg --latest
}
if ($LASTEXITCODE -ne 0) { throw "Release publish failed" }

# --------------------------------------------------- updater manifest -------
$winInstaller = $toSign | Select-Object -First 1
if ($winInstaller) {
    Write-Host "`nGenerating the updater manifest ..."
    Push-Location $repoRoot
    & bun scripts/generate-updater-manifest.ts --installer $winInstaller.FullName
    $manifest = Join-Path $repoRoot "src-tauri\target\release-artifacts\latest.json"
    if (Test-Path $manifest) {
        & $gh release upload "v$version" $manifest --repo $Repo --clobber
        Write-Host "  latest.json uploaded" -ForegroundColor Green
    }
    Pop-Location
}

Write-Host "`nReleased: https://github.com/$Repo/releases/tag/v$version" -ForegroundColor Green
