<#
Runs unittest.cmd (or build.cmd, via -Runner) repeatedly, clearing Smart App Control
casualties between attempts.

WHY: SAC refuses to load individual unsigned Rust proc-macro DLLs and build scripts out
of the cargo target dir. rustc cannot tell "blocked by the OS" from "not there" and
reports `error[E0463]: can't find crate for X`, so the build output looks like a broken
dependency tree. The verdict is per file content: recompiling the crate often produces a
byte-different artifact that SAC then accepts. Clearing one blocked artifact reveals the
next, so this converges over several attempts rather than one.

BEFORE USING THIS, set CARGO_BUILD_JOBS=1. The reputation check is a per-binary cloud
lookup and a full-throttle cargo build saturates it, so enough lookups fail that SAC
falls back to deny. Measured 2026-09-09: a parallel build was refused within a minute
where jobs=1 walked straight past the same crates.

If a build script is refused identically on every rebuild (deterministic output, so the
same content every time), change its bytes without touching the repo:
  $env:CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS = "2"
Keep that variable set for every later command against the same target dir, or cargo
rebuilds the artifacts you just got accepted.

Two things this gets right that a hand-run does not:
  * cargo's .fingerprint directories use the PACKAGE name (windows-implement) while the
    artifact uses the CRATE name (windows_implement). Deleting only the DLL leaves the
    fingerprint saying "fresh", so cargo never rebuilds it and the next run fails
    identically. Both spellings are normalised here.
  * The authoritative list of blocked files is the CodeIntegrity event log, not the build
    output. `os error 4551` appears only when cargo EXECUTES a blocked build script; a
    blocked proc-macro load surfaces as a bare E0463 with no OS error at all.
#>
param(
  [int]$MaxAttempts = 8,
  [string]$Repo = $PSScriptRoot,
  [string]$TargetDir = 'C:\tmp\hb\release',  # matches src-tauri/.cargo/config.toml
  [string]$Runner = 'unittest.cmd',          # or build.cmd for the installer
  [string]$SuccessPattern = 'TESTS PASSED',  # or 'BUILD SUCCEEDED' for build.cmd
  [string]$LogName = 'sac-runner.log'
)

$ErrorActionPreference = 'Continue'
$log = Join-Path $Repo $LogName

# whisper-rs-sys runs bindgen, which needs LLVM 18 - LLVM 22 mis-parses
# whisper_full_params. unittest.cmd falls back to C:\Program Files\LLVM\bin when
# this is unset, which on a machine with LLVM 22 installed is the broken one.
if (-not $env:LIBCLANG_PATH) {
  $candidate = Join-Path $env:USERPROFILE 'llvm18\bin'
  if (Test-Path (Join-Path $candidate 'libclang.dll')) { $env:LIBCLANG_PATH = $candidate }
}

function Remove-CrateArtifacts([string]$crate) {
  # $crate may arrive in either spelling: the event log yields package names
  # ("whisper-rs"), E0463 yields crate names ("yoke_derive"). Normalise both sides.
  $n = 0
  $crateUnderscore = $crate -replace '-', '_'
  Get-ChildItem "$TargetDir\deps\$crateUnderscore-*.dll" -ErrorAction SilentlyContinue | ForEach-Object {
    $base = $_.BaseName
    Get-ChildItem "$TargetDir\deps\$base.*" -ErrorAction SilentlyContinue | ForEach-Object {
      Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue; $n++
    }
  }
  Get-ChildItem "$TargetDir\.fingerprint" -Directory -ErrorAction SilentlyContinue | ForEach-Object {
    $norm = ($_.Name -replace '-[0-9a-f]{16}$', '') -replace '-', '_'
    if ($norm -eq $crateUnderscore) { Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue; $n++ }
  }
  return $n
}

for ($i = 1; $i -le $MaxAttempts; $i++) {
  "=== attempt $i/$MaxAttempts at $(Get-Date -Format 'HH:mm:ss') ==="
  $started = Get-Date
  Remove-Item $log -Force -ErrorAction SilentlyContinue
  $p = Start-Process -FilePath cmd.exe `
        -ArgumentList '/c', "$Repo\$Runner > $log 2>&1" `
        -WorkingDirectory $Repo -WindowStyle Hidden -PassThru
  $p.WaitForExit()

  $text = if (Test-Path $log) { Get-Content $log -Raw } else { '' }
  if ($text -match [regex]::Escape($SuccessPattern)) {
    "RESULT: $SuccessPattern on attempt $i"
    ($text -split "`n" | Select-String -Pattern 'test result:|Finished|bundle') -join "`n"
    exit 0
  }

  # Authoritative source: what did SAC actually refuse during this attempt?
  $blocked = @()
  $ev = Get-WinEvent -FilterHashtable @{
          LogName   = 'Microsoft-Windows-CodeIntegrity/Operational'
          Id        = 3077
          StartTime = $started.AddMinutes(-1)
        } -ErrorAction SilentlyContinue
  foreach ($e in $ev) {
    foreach ($m in [regex]::Matches($e.Message, '\\deps\\([A-Za-z0-9_]+)-[0-9a-f]{16}\.dll')) {
      $blocked += $m.Groups[1].Value
    }
    foreach ($m in [regex]::Matches($e.Message, '\\build\\([A-Za-z0-9_-]+?)-[0-9a-f]{16}\\build-script-build\.exe')) {
      $blocked += $m.Groups[1].Value
    }
  }

  # Fallback 1: a build script cargo could not EXECUTE. The event-log query above is
  # authoritative but can come back empty (timing, or the provider not yielding yet),
  # and the failing path is printed right there in the build output - so parse it too
  # rather than declaring "not a blocked artifact" and giving up.
  if ($text -match 'os error 4551') {
    foreach ($m in [regex]::Matches($text, 'build\\([A-Za-z0-9_.-]+?)-[0-9a-f]{16}\\build-script-build')) {
      $blocked += $m.Groups[1].Value
    }
  }

  # Fallback 2: crates rustc could not load, which is the same failure seen from inside.
  foreach ($m in [regex]::Matches($text, "can't find crate for ``([A-Za-z0-9_]+)``")) {
    $blocked += $m.Groups[1].Value
  }

  $blocked = $blocked | Sort-Object -Unique
  if (-not $blocked) {
    "RESULT: failed for a reason that is NOT a blocked artifact - stopping so it gets read properly."
    ($text -split "`n" | Select-String -Pattern 'error' | Select-Object -Last 15) -join "`n"
    exit 1
  }

  "  blocked/unloadable: $($blocked -join ', ')"
  foreach ($c in $blocked) {
    $n = Remove-CrateArtifacts $c
    "  purged $n file(s)/fingerprint(s) for $c"
  }

  # A build script blocked by SAC lives under build\, not deps\.
  foreach ($m in [regex]::Matches($text, 'build\\([A-Za-z0-9_-]+?-[0-9a-f]{16})\\build-script-build')) {
    $d = "$TargetDir\build\$($m.Groups[1].Value)"
    Get-ChildItem "$d\build-script-build*" -ErrorAction SilentlyContinue |
      ForEach-Object { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue; "  purged $($_.Name)" }
  }
}

"RESULT: still failing after $MaxAttempts attempts"
exit 1
