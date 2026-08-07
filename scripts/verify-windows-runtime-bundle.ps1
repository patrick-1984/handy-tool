[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$InstallerPath,
    [string]$Architecture,
    [string]$AuditPath,
    [string]$ApplicationExecutable = "handy.exe"
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

function Get-RelativePath {
    param(
        [Parameter(Mandatory)][string]$BasePath,
        [Parameter(Mandatory)][string]$Path
    )

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $baseWithSeparator = [System.IO.Path]::GetFullPath($BasePath).TrimEnd("\", "/") + $separator
    $baseUri = [uri]::new($baseWithSeparator)
    $pathUri = [uri]::new([System.IO.Path]::GetFullPath($Path))
    return [uri]::UnescapeDataString($baseUri.MakeRelativeUri($pathUri).ToString()).Replace("/", $separator)
}

if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
    throw "Installer not found: $InstallerPath"
}
$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path

if ([string]::IsNullOrWhiteSpace($AuditPath)) {
    $AuditPath = Join-Path $PSScriptRoot "..\src-tauri\vc-runtime-staging\vc-runtime-audit.json"
}
if (-not (Test-Path -LiteralPath $AuditPath -PathType Leaf)) {
    throw "Runtime staging audit not found: $AuditPath"
}
$AuditPath = (Resolve-Path -LiteralPath $AuditPath).Path
$audit = Get-Content -LiteralPath $AuditPath -Raw | ConvertFrom-Json

if ([string]::IsNullOrWhiteSpace($Architecture)) {
    $Architecture = $audit.TargetArchitecture
}
if ($Architecture -notin @("x86_64", "aarch64")) {
    throw "Unsupported or missing Windows target architecture '$Architecture'. Expected 'x86_64' or 'aarch64'."
}
if ($audit.TargetArchitecture -ne $Architecture) {
    throw "Staging audit architecture '$($audit.TargetArchitecture)' does not match requested bundle architecture '$Architecture'."
}

$expectedMachine = if ($Architecture -eq "x86_64") { 0x8664 } else { 0xAA64 }
$expectedMachineName = if ($Architecture -eq "x86_64") { "AMD64" } else { "ARM64" }
$auditFiles = @($audit.Files)
if ($auditFiles.Count -eq 0) {
    throw "The staging audit contains no runtime files: $AuditPath"
}

$missingAuditRequirements = @($requiredRuntimeFiles | Where-Object { $_ -notin $auditFiles.Name })
if ($missingAuditRequirements.Count -gt 0) {
    throw "The staging audit is missing required runtime files: $($missingAuditRequirements -join ', ')"
}

$sevenZipCommand = Get-Command 7z.exe, 7zz.exe, 7z, 7zz -ErrorAction SilentlyContinue | Select-Object -First 1
$sevenZipPath = if ($null -ne $sevenZipCommand) { $sevenZipCommand.Source } else { $null }
if ([string]::IsNullOrWhiteSpace($sevenZipPath)) {
    $standardSevenZipPaths = @(
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)) "7-Zip\7z.exe"),
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFilesX86)) "7-Zip\7z.exe")
    )
    $sevenZipPath = $standardSevenZipPaths |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
}
if ([string]::IsNullOrWhiteSpace($sevenZipPath)) {
    throw "7-Zip is required to inspect the NSIS installer, but no 7z/7zz executable was found on PATH."
}

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$extractRoot = Join-Path $tempBase ("handy-nsis-verify-{0}" -f [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $extractRoot | Out-Null

try {
    & $sevenZipPath x $InstallerPath "-o$extractRoot" -aoa | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "7-Zip failed to extract '$InstallerPath' (exit code $LASTEXITCODE)."
    }

    $applicationExecutables = @(Get-ChildItem -LiteralPath $extractRoot -Recurse -File -Filter $ApplicationExecutable)
    if ($applicationExecutables.Count -ne 1) {
        throw "Expected exactly one '$ApplicationExecutable' in the NSIS payload, found $($applicationExecutables.Count)."
    }

    $appRoot = $applicationExecutables[0].Directory.FullName
    $applicationMachine = Get-PeMachine -Path $applicationExecutables[0].FullName
    if ($applicationMachine -ne $expectedMachine) {
        throw ("Wrong-architecture application executable '{0}': expected {1} (0x{2:X4}), found 0x{3:X4}." -f `
            $ApplicationExecutable, $expectedMachineName, $expectedMachine, $applicationMachine)
    }
    $applicationHash = (Get-FileHash -LiteralPath $applicationExecutables[0].FullName -Algorithm SHA256).Hash

    $verifiedRuntimeFiles = @()
    foreach ($fileAudit in $auditFiles) {
        $payloadPath = Join-Path $appRoot $fileAudit.Name
        if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf)) {
            throw "Runtime DLL '$($fileAudit.Name)' is not beside '$ApplicationExecutable' in the NSIS payload."
        }

        $payloadHash = (Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256).Hash
        if ($payloadHash -ne $fileAudit.Sha256) {
            throw "Runtime DLL '$($fileAudit.Name)' does not match the staged SHA-256 hash."
        }

        $machine = Get-PeMachine -Path $payloadPath
        if ($machine -ne $expectedMachine) {
            throw ("Wrong-architecture payload file '{0}': expected {1} (0x{2:X4}), found 0x{3:X4}." -f `
                $fileAudit.Name, $expectedMachineName, $expectedMachine, $machine)
        }

        $signature = Get-AuthenticodeSignature -LiteralPath $payloadPath
        if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
            throw "Payload runtime file '$($fileAudit.Name)' does not have a valid Authenticode signature (status: $($signature.Status))."
        }
        if ($null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch "O=Microsoft Corporation(?:,|$)") {
            throw "Payload runtime file '$($fileAudit.Name)' is not signed by Microsoft Corporation."
        }

        $payloadFile = Get-Item -LiteralPath $payloadPath
        if ($payloadFile.VersionInfo.FileVersion -ne $fileAudit.FileVersion) {
            throw "Payload runtime file '$($fileAudit.Name)' has version '$($payloadFile.VersionInfo.FileVersion)', expected '$($fileAudit.FileVersion)'."
        }

        $verifiedRuntimeFiles += [pscustomobject]@{
            Name = $fileAudit.Name
            FileVersion = $payloadFile.VersionInfo.FileVersion
            Sha256 = $payloadHash
            PeMachine = ("0x{0:X4}" -f $machine)
            SignerThumbprint = $signature.SignerCertificate.Thumbprint
        }
    }

    foreach ($requiredFile in $requiredRuntimeFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $appRoot $requiredFile) -PathType Leaf)) {
            throw "Required runtime DLL '$requiredFile' is missing beside '$ApplicationExecutable'."
        }
    }

    $resourceSource = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\src-tauri\resources"))
    $sourceResources = @(Get-ChildItem -LiteralPath $resourceSource -Recurse -File)
    foreach ($sourceResource in $sourceResources) {
        $relativePath = Get-RelativePath -BasePath $resourceSource -Path $sourceResource.FullName
        $payloadResource = Join-Path (Join-Path $appRoot "resources") $relativePath
        if (-not (Test-Path -LiteralPath $payloadResource -PathType Leaf)) {
            throw "Existing Tauri resource '$relativePath' is missing from the NSIS payload."
        }

        $sourceHash = (Get-FileHash -LiteralPath $sourceResource.FullName -Algorithm SHA256).Hash
        $payloadResourceHash = (Get-FileHash -LiteralPath $payloadResource -Algorithm SHA256).Hash
        if ($sourceHash -ne $payloadResourceHash) {
            throw "Existing Tauri resource '$relativePath' changed in the NSIS payload."
        }
    }

    $relativeApplicationPath = Get-RelativePath -BasePath $extractRoot -Path $applicationExecutables[0].FullName
    $verification = [ordered]@{
        SchemaVersion = 1
        VerifiedAtUtc = [DateTime]::UtcNow.ToString("o")
        InstallerPath = $InstallerPath
        InstallerSha256 = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash
        TargetArchitecture = $Architecture
        ApplicationPathInPayload = $relativeApplicationPath
        ApplicationSha256 = $applicationHash
        ApplicationPeMachine = ("0x{0:X4}" -f $applicationMachine)
        ResourceFilesVerified = $sourceResources.Count
        RequiredFiles = $requiredRuntimeFiles
        RuntimeFiles = $verifiedRuntimeFiles
    }

    $verificationPath = "$InstallerPath.runtime-verification.json"
    $verification | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $verificationPath -Encoding utf8

    Write-Host "Verified NSIS app-local VC++ runtime payload:"
    Write-Host "  Installer: $InstallerPath"
    Write-Host "  Application: $relativeApplicationPath"
    Write-Host "  Runtime DLLs: $($verifiedRuntimeFiles.Count)"
    Write-Host "  Existing resources: $($sourceResources.Count)"
    Write-Host "  Audit: $verificationPath"
}
finally {
    $resolvedExtractRoot = [System.IO.Path]::GetFullPath($extractRoot)
    if ($resolvedExtractRoot.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedExtractRoot) -like "handy-nsis-verify-*") {
        [System.IO.Directory]::Delete($resolvedExtractRoot, $true)
    }
}
