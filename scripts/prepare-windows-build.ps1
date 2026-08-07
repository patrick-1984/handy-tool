[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$stageScript = Join-Path $PSScriptRoot "stage-vc-runtime.ps1"

Write-Host "Preparing the Windows Tauri build..."
& $stageScript

Push-Location $repositoryRoot
try {
    $bun = Get-Command bun -ErrorAction Stop
    & $bun.Source run build
    if ($LASTEXITCODE -ne 0) {
        throw "The frontend build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
