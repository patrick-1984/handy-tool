[CmdletBinding()]
param(
    [string]$Architecture = $env:TAURI_ENV_ARCH,
    [string]$Destination,
    [string]$VisualStudioInstallPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$requiredRuntimeFiles = @(
    "msvcp140.dll",
    "msvcp140_1.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll"
)

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [System.IO.File]::Open(
        $Path,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read
    )
    $reader = [System.IO.BinaryReader]::new($stream)

    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "'$Path' does not have a valid DOS executable header."
        }

        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        if ($peOffset -gt ($stream.Length - 6)) {
            throw "'$Path' has an invalid PE header offset."
        }

        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "'$Path' does not have a valid PE signature."
        }

        return $reader.ReadUInt16()
    }
    finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

function Add-VisualStudioRoot {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][System.Collections.Generic.List[string]]$Roots,
        [Parameter(Mandatory)][AllowEmptyCollection()][System.Collections.Generic.HashSet[string]]$Seen,
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Container)) {
        return
    }

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if ($Seen.Add($resolved)) {
        $Roots.Add($resolved)
    }
}

if ($Architecture -notin @("x86_64", "aarch64")) {
    throw "Unsupported or missing Windows target architecture '$Architecture'. Expected 'x86_64' or 'aarch64'."
}

$architectureInfo = if ($Architecture -eq "x86_64") {
    [pscustomobject]@{ RedistDirectory = "x64"; PeMachine = 0x8664; PeMachineName = "AMD64" }
}
else {
    [pscustomobject]@{ RedistDirectory = "arm64"; PeMachine = 0xAA64; PeMachineName = "ARM64" }
}

if ([string]::IsNullOrWhiteSpace($Destination)) {
    $Destination = Join-Path $PSScriptRoot "..\src-tauri\vc-runtime-staging"
}
$Destination = [System.IO.Path]::GetFullPath($Destination)

$installRoots = [System.Collections.Generic.List[string]]::new()
$seenRoots = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

if (-not [string]::IsNullOrWhiteSpace($VisualStudioInstallPath)) {
    if (-not (Test-Path -LiteralPath $VisualStudioInstallPath -PathType Container)) {
        throw "The supplied Visual Studio installation path does not exist: $VisualStudioInstallPath"
    }
    Add-VisualStudioRoot -Roots $installRoots -Seen $seenRoots -Path $VisualStudioInstallPath
}
else {
    Add-VisualStudioRoot -Roots $installRoots -Seen $seenRoots -Path $env:VSINSTALLDIR

    $programFilesX86 = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFilesX86)
    $vswhere = Join-Path $programFilesX86 "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere -PathType Leaf) {
        $discoveredRoots = @(& $vswhere -all -products * -property installationPath)
        if ($LASTEXITCODE -ne 0) {
            throw "vswhere failed while locating Visual Studio installations (exit code $LASTEXITCODE)."
        }
        foreach ($root in ($discoveredRoots | Sort-Object)) {
            Add-VisualStudioRoot -Roots $installRoots -Seen $seenRoots -Path $root
        }
    }

    $visualStudioBase = Join-Path $programFilesX86 "Microsoft Visual Studio"
    if (Test-Path -LiteralPath $visualStudioBase -PathType Container) {
        foreach ($versionDirectory in (Get-ChildItem -LiteralPath $visualStudioBase -Directory | Sort-Object Name)) {
            foreach ($editionDirectory in (Get-ChildItem -LiteralPath $versionDirectory.FullName -Directory | Sort-Object Name)) {
                Add-VisualStudioRoot -Roots $installRoots -Seen $seenRoots -Path $editionDirectory.FullName
            }
        }
    }
}

if ($installRoots.Count -eq 0) {
    throw "No Visual Studio installation was found. Install the MSVC redistributable component or pass -VisualStudioInstallPath."
}

$crtCandidates = @()
foreach ($installRoot in $installRoots) {
    $redistRoot = Join-Path $installRoot "VC\Redist\MSVC"
    if (-not (Test-Path -LiteralPath $redistRoot -PathType Container)) {
        continue
    }

    foreach ($redistDirectory in (Get-ChildItem -LiteralPath $redistRoot -Directory)) {
        $redistVersion = $null
        if (-not [version]::TryParse($redistDirectory.Name, [ref]$redistVersion)) {
            continue
        }

        $architectureRoot = Join-Path $redistDirectory.FullName $architectureInfo.RedistDirectory
        if (-not (Test-Path -LiteralPath $architectureRoot -PathType Container)) {
            continue
        }

        foreach ($crtDirectory in (Get-ChildItem -LiteralPath $architectureRoot -Directory -Filter "Microsoft.VC*.CRT")) {
            if ($crtDirectory.Name -like "*Debug*") {
                continue
            }

            $crtCandidates += [pscustomobject]@{
                InstallRoot = $installRoot
                RedistVersion = $redistVersion
                CrtName = $crtDirectory.Name
                Source = $crtDirectory.FullName
            }
        }
    }
}

if ($crtCandidates.Count -eq 0) {
    throw "No $($architectureInfo.RedistDirectory) Microsoft.VC*.CRT folder was found under a Visual Studio VC\Redist tree."
}

$selected = $crtCandidates |
    Sort-Object `
        @{ Expression = { $_.RedistVersion }; Descending = $true },
        @{ Expression = { $_.CrtName }; Descending = $true },
        @{ Expression = { $_.Source }; Descending = $false } |
    Select-Object -First 1

$sourceFiles = @(Get-ChildItem -LiteralPath $selected.Source -File -Filter "*.dll" | Sort-Object Name)
if ($sourceFiles.Count -eq 0) {
    throw "The selected CRT directory contains no DLLs: $($selected.Source)"
}

$missingRequired = @($requiredRuntimeFiles | Where-Object { $_ -notin $sourceFiles.Name })
if ($missingRequired.Count -gt 0) {
    throw "The selected CRT directory is incomplete. Missing: $($missingRequired -join ', '). Source: $($selected.Source)"
}

$auditFiles = @()
foreach ($file in $sourceFiles) {
    $machine = Get-PeMachine -Path $file.FullName
    if ($machine -ne $architectureInfo.PeMachine) {
        throw ("Wrong-architecture runtime file '{0}': expected {1} (0x{2:X4}), found 0x{3:X4}." -f `
            $file.FullName, $architectureInfo.PeMachineName, $architectureInfo.PeMachine, $machine)
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Runtime file '$($file.FullName)' does not have a valid Authenticode signature (status: $($signature.Status))."
    }
    if ($null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch "O=Microsoft Corporation(?:,|$)") {
        throw "Runtime file '$($file.FullName)' is not signed by Microsoft Corporation."
    }

    $fileVersion = $file.VersionInfo.FileVersion
    if ([string]::IsNullOrWhiteSpace($fileVersion)) {
        throw "Runtime file '$($file.FullName)' has no file version."
    }

    $auditFiles += [pscustomobject]@{
        Name = $file.Name
        FileVersion = $fileVersion
        ProductVersion = $file.VersionInfo.ProductVersion
        Sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        PeMachine = ("0x{0:X4}" -f $machine)
        SignatureStatus = $signature.Status.ToString()
        SignerSubject = $signature.SignerCertificate.Subject
        SignerThumbprint = $signature.SignerCertificate.Thumbprint
    }
}

$runtimeVersions = @($auditFiles.FileVersion | Sort-Object -Unique)
if ($runtimeVersions.Count -ne 1) {
    throw "The selected CRT directory contains mixed file versions: $($runtimeVersions -join ', '). Source: $($selected.Source)"
}

New-Item -ItemType Directory -Path $Destination -Force | Out-Null
Get-ChildItem -LiteralPath $Destination -File -Filter "*.dll" -ErrorAction SilentlyContinue |
    Remove-Item -Force
$auditPath = Join-Path $Destination "vc-runtime-audit.json"
if (Test-Path -LiteralPath $auditPath -PathType Leaf) {
    Remove-Item -LiteralPath $auditPath -Force
}

foreach ($file in $sourceFiles) {
    Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $Destination $file.Name)
}

foreach ($file in $auditFiles) {
    $stagedPath = Join-Path $Destination $file.Name
    $stagedHash = (Get-FileHash -LiteralPath $stagedPath -Algorithm SHA256).Hash
    if ($stagedHash -ne $file.Sha256) {
        throw "Hash mismatch after staging '$($file.Name)'."
    }
}

$audit = [ordered]@{
    SchemaVersion = 1
    GeneratedAtUtc = [DateTime]::UtcNow.ToString("o")
    TargetArchitecture = $Architecture
    PeMachine = ("0x{0:X4}" -f $architectureInfo.PeMachine)
    VisualStudioInstallPath = $selected.InstallRoot
    RedistVersion = $selected.RedistVersion.ToString()
    CrtDirectoryName = $selected.CrtName
    RuntimeFileVersion = $runtimeVersions[0]
    SourceDirectory = $selected.Source
    DestinationDirectory = $Destination
    RequiredFiles = $requiredRuntimeFiles
    Files = $auditFiles
}

$audit | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $auditPath -Encoding utf8

Write-Host "Staged $($sourceFiles.Count) signed $Architecture VC++ runtime DLLs from:"
Write-Host "  $($selected.Source)"
Write-Host "Destination: $Destination"
Write-Host "Audit: $auditPath"
