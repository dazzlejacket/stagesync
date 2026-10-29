<#
.SYNOPSIS
    Build the StageSync desktop application and package it for Windows.

.DESCRIPTION
    Builds the `stagesync-desktop` binary with cargo, copies it into a portable
    folder together with a README that explains the Microsoft Visual C++
    runtime requirement, zips that folder, and - when NSIS is available -
    builds a per-user installer from `packaging/windows/installer.nsi`.

    Every artefact is written under -OutputDir and its absolute path is printed
    when the script is done.

    The installer step is skipped with a clear message when `makensis` is not
    on PATH. That is not a failure: the portable ZIP is a complete artefact on
    its own. The skip is always reported, never silent.

.PARAMETER Profile
    The cargo profile to build. `release` (the default) builds the optimised
    binary; any other value is passed to cargo as `--profile <value>`.

.PARAMETER OutputDir
    The directory the artefacts are written to, relative to the repository
    root unless it is already absolute. Defaults to `dist/windows`.

.EXAMPLE
    ./packaging/windows/build.ps1
    ./packaging/windows/build.ps1 -Profile release -OutputDir dist/windows
#>
[CmdletBinding()]
param(
    [string]$Profile = 'release',
    [string]$OutputDir = 'dist/windows'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Locate the repository root -------------------------------------------------

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..' | Join-Path -ChildPath '..')).Path
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'Cargo.toml'))) {
    throw "the repository root could not be located from '$PSScriptRoot'"
}
Push-Location $repoRoot
try {
    & cargo build --version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "cargo is not available on PATH"
    }

    # --- The version ------------------------------------------------------------

    $version = $null
    $inWorkspacePackage = $false
    foreach ($line in (Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml'))) {
        if ($line -match '^\s*\[workspace\.package\]') {
            $inWorkspacePackage = $true
            continue
        }
        if ($inWorkspacePackage -and $line -match '^\s*\[') {
            break
        }
        if ($inWorkspacePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
            $version = $Matches[1]
            break
        }
    }
    if (-not $version) {
        throw 'the workspace version could not be read from Cargo.toml'
    }

    # The numeric version the Windows file properties need: four dot-separated
    # numbers, without any pre-release or build metadata.
    $numericParts = (($version -split '[-+]')[0] -split '\.') | Where-Object { $_ -match '^\d+$' }
    while ($numericParts.Count -lt 4) { $numericParts = @($numericParts) + '0' }
    $versionNumeric = (($numericParts | Select-Object -First 4) -join '.')

    # --- The output directory ---------------------------------------------------

    if ([System.IO.Path]::IsPathRooted($OutputDir)) {
        $outputPath = $OutputDir
    }
    else {
        $outputPath = Join-Path $repoRoot $OutputDir
    }
    $outputPath = [System.IO.Path]::GetFullPath($outputPath)
    $portableDir = Join-Path $outputPath 'portable'
    New-Item -ItemType Directory -Force -Path $portableDir | Out-Null

    # --- Build ------------------------------------------------------------------

    Write-Host "==> building the stagesync-desktop binary (profile: $Profile)"
    if ($Profile -eq 'release') {
        & cargo build --release --locked --bin stagesync-desktop
    }
    else {
        & cargo build --profile $Profile --locked --bin stagesync-desktop
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $binaryName = if ($env:OS -eq 'Windows_NT') { 'stagesync-desktop.exe' } else { 'stagesync-desktop' }
    $builtBinary = Join-Path $repoRoot (Join-Path 'target' (Join-Path $Profile $binaryName))
    if (-not (Test-Path -LiteralPath $builtBinary)) {
        throw "the built binary was not found at '$builtBinary'"
    }

    # --- The portable folder ----------------------------------------------------

    $portableBinary = Join-Path $portableDir $binaryName
    Copy-Item -LiteralPath $builtBinary -Destination $portableBinary -Force

    $readmePath = Join-Path $portableDir 'README.txt'
    $readme = @"
StageSync $version - portable Windows build
============================================

This folder contains
--------------------
  $binaryName   the StageSync desktop application

The Microsoft Visual C++ runtime is required
---------------------------------------------
This build is compiled with the Microsoft Visual C++ toolchain, and it links
the C runtime (the "CRT") dynamically. Windows therefore needs the Microsoft
Visual C++ Redistributable to be installed before StageSync will start.
Without it the application refuses to start and Windows reports a missing
DLL, for example:

    The code execution cannot proceed because VCRUNTIME140_1.dll was not found.

How to get the runtime
----------------------
  * The official Microsoft download for 64-bit Windows:
        https://aka.ms/vs/17/release/vc_redist.x64.exe
    Run it once; it installs the redistributable for the machine. Microsoft
    allows the redistributable to be redistributed, but it is not bundled in
    this ZIP.
  * Or install the "Microsoft Visual C++ Redistributable" entry that is
    already present on any machine with Visual Studio or the Visual Studio
    Build Tools installed (in "Apps & features" on Windows 11).

The runtime is needed once per machine, not once per application.

What StageSync does with your files
-----------------------------------
StageSync never modifies a recording it is given. It reads the reference and
the source recordings you point it at, and writes the drift-corrected audio to
a directory you choose.
"@
    Set-Content -LiteralPath $readmePath -Value $readme -Encoding UTF8

    # --- The portable ZIP -------------------------------------------------------

    $zipPath = Join-Path $outputPath "stagesync-$version-windows-x64-portable.zip"
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force
    }
    Write-Host '==> zipping the portable folder'
    Compress-Archive -Path $portableDir -DestinationPath $zipPath -CompressionLevel Optimal

    # --- The installer ----------------------------------------------------------

    $setupPath = Join-Path $outputPath "stagesync-$version-windows-x64-setup.exe"
    $nsiPath = Join-Path $repoRoot (Join-Path 'packaging' (Join-Path 'windows' 'installer.nsi'))
    if (-not (Test-Path -LiteralPath $nsiPath)) {
        throw "the NSIS script was not found at '$nsiPath'"
    }

    $makensis = Get-Command 'makensis' -ErrorAction SilentlyContinue
    if (-not $makensis) {
        foreach ($candidate in @(
                (Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'),
                (Join-Path $env:ProgramFiles 'NSIS\makensis.exe'))) {
            if ($candidate -and (Test-Path -LiteralPath $candidate)) {
                $makensis = $candidate
                break
            }
        }
    }

    if (-not $makensis) {
        Write-Host ''
        Write-Warning 'NSIS (makensis) was not found on PATH, so the installer step was SKIPPED.'
        Write-Warning 'Only the portable ZIP was produced by this run.'
        Write-Warning 'Install NSIS (https://nsis.sourceforge.io/Main.html) and run this script again'
        Write-Warning 'to produce the setup executable as well.'
    }
    else {
        $installDir = Join-Path $env:LOCALAPPDATA 'Programs\StageSync'
        Write-Host '==> building the NSIS installer'
        # makensis writes its output file into the current directory, so the
        # script is run from the output directory.
        Push-Location $outputPath
        try {
            & $makensis `
                "/DSTAGESYNC_VERSION=$version" `
                "/DSTAGESYNC_VERSION_NUM=$versionNumeric" `
                "/DSTAGESYNC_INSTALL_DIR=$installDir" `
                "/DSTAGESYNC_EXE=$builtBinary" `
                "/DSTAGESYNC_PORTABLE_ZIP=$zipPath" `
                $nsiPath
            if ($LASTEXITCODE -ne 0) {
                throw "makensis failed with exit code $LASTEXITCODE"
            }
        }
        finally {
            Pop-Location
        }
    }

    # --- What was written -------------------------------------------------------

    Write-Host ''
    Write-Host 'StageSync Windows packaging finished.'
    Write-Host "version           : $version"
    Write-Host "profile           : $Profile"
    Write-Host "portable binary   : $([System.IO.Path]::GetFullPath($portableBinary))"
    Write-Host "portable readme   : $([System.IO.Path]::GetFullPath($readmePath))"
    Write-Host "portable zip      : $([System.IO.Path]::GetFullPath($zipPath))"
    if (Test-Path -LiteralPath $setupPath) {
        Write-Host "installer         : $([System.IO.Path]::GetFullPath($setupPath))"
    }
    else {
        Write-Host "installer         : (not produced - NSIS was not available)"
    }
    Write-Host "output directory  : $outputPath"
}
finally {
    Pop-Location
}
