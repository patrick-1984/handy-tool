<#
Runs unittest.cmd repeatedly, clearing Smart App Control casualties between attempts.

WHY: SAC (enforced on this machine, policy {0283ac0f-...}) refuses to load individual
unsigned Rust proc-macro DLLs out of the cargo target dir. rustc cannot tell "blocked by
the OS" from "not there" and reports `error[E0463]: can't find crate for X`, so the build
output looks like a broken dependency tree. The verdict is per file hash: recompiling the
same crate usually produces a byte-different DLL that SAC then accepts. Clearing one
blocked artifact reveals the next, so this converges over several attempts rather than one.

Two things this gets right that a hand-run does not:
  * cargo's .fingerprint directories use the PACKAGE name (windows-implement) while the
    artifact uses the CRATE name (windows_implement). Deleting only the DLL leaves the
    fingerprint saying "fresh", so cargo never rebuilds it and the next run fails
    identically. Both spellings are matched here.
  * The authoritative list of blocked files is the CodeIntegrity event log, not the build
    output. The log is consulted first; crate names parsed out of E0463 are the fallback.
#>
param(
  [int]$MaxAttempts = 8,
  [string]$Repo = $PSScriptRoot,
  [string]$TargetDir = 'C:\tmp\hb\release'   # matches src-tauri/.cargo/config.toml
)

$ErrorActionPreference = 'Continue'
$log = Join-Path $Repo 'unittest-140.log'

# whisper-rs-sys runs bindgen, which needs LLVM 18 - LLVM 22 mis-parses
# whisper_full_params. unittest.cmd falls back to C:\Program Files\LLVM\bin when
# this is unset, which on a machine with LLVM 22 installed is the broken one.
if (-not $env:LIBCLANG_PATH) {
  $candidate = Join-Path $env:USERPROFILE 'llvm18\bin'
  if (Test-Path (Join-Path $candidate 'libclang.dll')) { $env:LIBCLANG_PATH = $candidate }
}

function Remove-CrateArtifacts([string]$crate) {
  # $crate is the crate name (underscores). Purge every metadata-hash variant of the
  # artifact plus every fingerprint dir, trying both hyphen and underscore spellings.
  $n = 0
  $crateUnderscore = $crate -replace '-','_'
  Get-ChildItem "$TargetDir\deps\$crateUnderscore-*.dll" -ErrorAction SilentlyContinue | ForEach-Object {
    $base = $_.BaseName
    Get-ChildItem "$TargetDir\deps\$base.*" -ErrorAction SilentlyContinue | ForEach-Object {
      Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue; $n++
    }
  }
  Get-ChildItem "$TargetDir\.fingerprint" -Directory -ErrorAction SilentlyContinue | ForEach-Object {
    $norm = ($_.Name -replace '-[0-9a-f]{16}$','') -replace '-','_'
    # Normalise BOTH sides: the caller may pass either spelling (the event log
    # yields package names like 'whisper-rs', E0463 yields crate names like 'yoke_derive').
    if ($norm -eq ($crate -replace '-','_')) { Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue; $n++ }
  }
  return $n
}

for ($i = 1; $i -le $MaxAttempts; $i++) {
  "=== attempt $i/$MaxAttempts at $(Get-Date -Format 'HH:mm:ss') ==="
  $started = Get-Date
  Remove-Item $log -Force -ErrorAction SilentlyContinue
  $p = Start-Process -FilePath cmd.exe `
        -ArgumentList '/c', "$Repo\unittest.cmd > $log 2>&1" `
        -WorkingDirectory $Repo -WindowStyle Hidden -PassThru
  $p.WaitForExit()

  $text = if (Test-Path $log) { Get-Content $log -Raw } else { '' }
  if ($text -match 'TESTS PASSED') {
    "RESULT: TESTS PASSED on attempt $i"
    ($text -split "`n" | Select-String -Pattern 'test result:') -join "`n"
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

  # Fallback: crates rustc could not load, which is the same failure seen from inside.
  foreach ($m in [regex]::Matches($text, "can't find crate for ``([A-Za-z0-9_]+)``")) {
    $blocked += $m.Groups[1].Value
  }

  $blocked = $blocked | Sort-Object -Unique
  if (-not $blocked) {
    "RESULT: build failed for a reason that is NOT a blocked artifact - stopping so it gets read properly."
    ($text -split "`n" | Select-String -Pattern '^error' | Select-Object -First 15) -join "`n"
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
