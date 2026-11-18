<#
.SYNOPSIS
    Build the StageSync desktop application and package it for Windows.

.DESCRIPTION
    Builds the `stagesync-desktop` binary with cargo, copies it into a portable
    folder together with the licence and notice bundle and a README that
    explains the Microsoft Visual C++ runtime requirement, zips that folder
    reproducibly, and - when NSIS is available - builds a per-user installer
    from `packaging/windows/installer.nsi`.

    Every artefact is written under -OutputDir and its absolute path is printed
    when the script is done.

    The installer step is skipped with a clear message when `makensis` is not
    on PATH. That is not a failure: the portable ZIP is a complete artefact on
    its own. The skip is always reported, never silent.

    Reproducibility: the ZIP's entries are written in a fixed order with a
    fixed timestamp (2020-01-01T00:00:00Z, or $env:SOURCE_DATE_EPOCH when it
    is set), and a SHA-256 manifest of the artefacts is written next to them.
    Two runs of this script on the same inputs produce the same bytes.

.PARAMETER Profile
    The cargo profile to build. `release` (the default) builds the optimised
    binary; any other value is passed to cargo as `--profile <value>`.

.PARAMETER OutputDir
    The directory the artefacts are written to, relative to the repository
    root unless it is already absolute. Defaults to `dist/windows`.

.PARAMETER SkipToolchainCheck
    Do not insist on the Rust toolchain pinned in `packaging/rust-toolchain.toml`.
    The check is on by default because a release built with a different
    compiler is not the release the project claims to ship.

.EXAMPLE
    ./packaging/windows/build.ps1
    ./packaging/windows/build.ps1 -Profile release -OutputDir dist/windows
#>
[CmdletBinding()]
param(
    [string]$Profile = 'release',
    [string]$OutputDir = 'dist/windows',
    [switch]$SkipToolchainCheck
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# The timestamp every archive member and every staged file is given. It is a
# constant, so the bytes of the artefacts do not depend on the clock.
$defaultEpoch = 1577836800   # 2020-01-01T00:00:00Z

function Invoke-NativeCommand {
    <#
        Run a native command and return its exit code.

        PowerShell 5.1 turns whatever a native command writes to stderr into
        a terminating error when $ErrorActionPreference is Stop - and cargo
        writes its progress to stderr. The preference is therefore relaxed
        around the call and the exit code is checked instead, which is what
        actually says whether the command worked.

        The command's output is written to the host and kept in
        $script:LastNativeOutput for the callers that want to read it.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments
    )
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & $Command @Arguments 2>&1
        $code = $LASTEXITCODE
        $script:LastNativeOutput = ($output | Out-String)
        $output | ForEach-Object { Write-Host $_ }
        return $code
    }
    finally {
        $ErrorActionPreference = $previous
    }
}

function Test-PythonExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$ExePath,
        [string[]]$Arguments = @('--version')
    )
    try {
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $ExePath
        $psi.Arguments = ($Arguments -join ' ')
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.RedirectStandardInput = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true

        $proc = [System.Diagnostics.Process]::Start($psi)
        if (-not $proc) {
            return $false
        }
        $proc.StandardInput.Close()
        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        if (-not $proc.WaitForExit(3000)) {
            try { $proc.Kill() } catch {}
            return $false
        }
        $combined = ($stdout + "`n" + $stderr).Trim()
        return ($proc.ExitCode -eq 0 -and $combined -match 'Python 3\.')
    }
    catch {
        return $false
    }
}

function Resolve-PythonCommand {
    <#
        Find a Python 3 interpreter. The notices generator needs one; the
        packaging step must not silently ship a stale notices bundle, so the
        script fails loudly when there is none.

        Every candidate on PATH (not just the first) is probed with a bounded
        timeout and closed stdin so broken Microsoft Store WindowsApps alias
        stubs never throw or stall the search before a real Python is tried.
    #>
    if ($env:STAGESYNC_PYTHON -and (Test-Path -LiteralPath $env:STAGESYNC_PYTHON)) {
        if (Test-PythonExecutable -ExePath $env:STAGESYNC_PYTHON) {
            return $env:STAGESYNC_PYTHON
        }
    }

    $storeCandidates = @()
    foreach ($name in @('python3', 'python', 'py')) {
        $candidates = Get-Command $name -All -CommandType Application -ErrorAction SilentlyContinue
        foreach ($found in $candidates) {
            $src = $found.Source
            if (-not $src) {
                continue
            }
            $isStoreAlias = $src -match '\\Microsoft\\WindowsApps\\python3?\.exe$'
            if ($isStoreAlias) {
                $storeCandidates += $src
                continue
            }
            $argsList = if ($name -eq 'py') { @('-3', '--version') } else { @('--version') }
            if (Test-PythonExecutable -ExePath $src -Arguments $argsList) {
                if ($name -eq 'py') {
                    try {
                        $realExe = (& $src -3 -c "import sys; print(sys.executable)" 2>$null | Out-String).Trim()
                        if ($LASTEXITCODE -eq 0 -and $realExe -and (Test-Path -LiteralPath $realExe)) {
                            return $realExe
                        }
                    }
                    catch {}
                }
                return $src
            }
        }
    }

    foreach ($root in @(
            (Join-Path $env:LOCALAPPDATA 'Programs\Python'),
            (Join-Path $env:ProgramFiles 'Python*')
        )) {
        foreach ($dir in (Get-Item -Path $root -ErrorAction SilentlyContinue)) {
            $exes = Get-ChildItem -Path $dir -Filter 'python.exe' -Recurse -Depth 2 -ErrorAction SilentlyContinue |
                Sort-Object FullName -Descending
            foreach ($exe in $exes) {
                if (Test-PythonExecutable -ExePath $exe.FullName) {
                    return $exe.FullName
                }
            }
        }
    }

    # Try WindowsApps aliases only if no standalone Python installation succeeded.
    foreach ($src in $storeCandidates) {
        if (Test-PythonExecutable -ExePath $src) {
            return $src
        }
    }

    return $null
}

function Set-FixedTimestamp {
    <#
        Give a file a fixed modification time, so that an archive built from
        it - and an installer that embeds it - carries the same timestamp on
        every machine.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][datetime]$Timestamp
    )
    [System.IO.File]::SetLastWriteTimeUtc($Path, $Timestamp)
}

function New-DeterministicZip {
    <#
        Write a ZIP archive with a fixed member order and a fixed member
        timestamp, so that the same input tree always produces the same bytes.

        `Compress-Archive` is not used: it stamps each member with the file's
        own modification time, which makes the archive depend on when the
        build ran.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$SourceDir,
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [Parameter(Mandatory = $true)][string]$RootName,
        [Parameter(Mandatory = $true)][datetime]$Timestamp
    )

    Add-Type -AssemblyName 'System.IO.Compression'
    Add-Type -AssemblyName 'System.IO.Compression.FileSystem'

    $sourceFull = (Resolve-Path -LiteralPath $SourceDir).Path.TrimEnd('\', '/')
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }

    $files = Get-ChildItem -LiteralPath $sourceFull -Recurse -File |
        Sort-Object -Property { $_.FullName.Substring($sourceFull.Length) }

    $stream = [System.IO.File]::Open($ZipPath, [System.IO.FileMode]::CreateNew)
    try {
        $archive = New-Object System.IO.Compression.ZipArchive($stream, [System.IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($file in $files) {
                $relative = $file.FullName.Substring($sourceFull.Length).TrimStart('\', '/') -replace '\\', '/'
                $entry = $archive.CreateEntry("$RootName/$relative", [System.IO.Compression.CompressionLevel]::Optimal)
                # ZipArchive stores the timestamp with a two-second resolution,
                # so the value is rounded to a whole even second.
                $entry.LastWriteTime = [System.DateTimeOffset]::new(
                    [datetime]::SpecifyKind($Timestamp, [System.DateTimeKind]::Utc))
                $entryStream = $entry.Open()
                try {
                    $input = [System.IO.File]::OpenRead($file.FullName)
                    try { $input.CopyTo($entryStream) }
                    finally { $input.Dispose() }
                }
                finally { $entryStream.Dispose() }
            }
        }
        finally { $archive.Dispose() }
    }
    finally { $stream.Dispose() }
}

# --- Locate the repository root -------------------------------------------------

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..' | Join-Path -ChildPath '..')).Path
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'Cargo.toml'))) {
    throw "the repository root could not be located from '$PSScriptRoot'"
}
Push-Location $repoRoot
try {
    # --- The toolchain ----------------------------------------------------------

    $cargoCommand = Get-Command 'cargo' -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $cargoCommand) {
        throw "cargo was not found on PATH. Install the Rust toolchain from https://rustup.rs and try again."
    }
    $cargo = $cargoCommand.Source

    # `cargo --version` is the availability check. `cargo build --version` is
    # not a thing: cargo rejects it, and the check used to report a perfectly
    # good toolchain as missing.
    $cargoExit = Invoke-NativeCommand $cargo '--version'
    if ($cargoExit -ne 0) {
        throw "cargo --version failed with exit code $cargoExit"
    }
    $cargoVersion = $script:LastNativeOutput.Trim()
    Write-Host "==> toolchain: $cargoVersion"

    $toolchainFile = Join-Path $repoRoot (Join-Path 'packaging' 'rust-toolchain.toml')
    if ((-not $SkipToolchainCheck) -and (Test-Path -LiteralPath $toolchainFile)) {
        $pinned = (Select-String -LiteralPath $toolchainFile -Pattern '^\s*channel\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
        $rustcCommand = Get-Command 'rustc' -CommandType Application -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $rustcCommand) {
            throw "rustc was not found on PATH, so the pinned toolchain ($pinned) could not be checked."
        }
        $rustcExit = Invoke-NativeCommand $rustcCommand.Source '--version'
        if ($rustcExit -ne 0) {
            throw "rustc --version failed with exit code $rustcExit"
        }
        $rustcVersion = ($script:LastNativeOutput.Trim() -replace '^rustc\s+', '')
        $installed = ($rustcVersion -split '\s')[0]
        if ($installed -ne $pinned) {
            throw ("the pinned Rust toolchain is $pinned but $installed is installed. " +
                "Install it with 'rustup toolchain install $pinned', or pass -SkipToolchainCheck " +
                "if you are deliberately building something else.")
        }
        Write-Host "==> the pinned toolchain ($pinned) is in use"
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
    if (Test-Path -LiteralPath $portableDir) {
        Remove-Item -LiteralPath $portableDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $portableDir | Out-Null

    # --- The fixed timestamp ----------------------------------------------------

    $epoch = $defaultEpoch
    if ($env:SOURCE_DATE_EPOCH) {
        $parsed = 0L
        if (-not [long]::TryParse($env:SOURCE_DATE_EPOCH, [ref]$parsed)) {
            throw "SOURCE_DATE_EPOCH is not an integer: '$($env:SOURCE_DATE_EPOCH)'"
        }
        $epoch = $parsed
    }
    $timestamp = [DateTime]::SpecifyKind([DateTimeOffset]::FromUnixTimeSeconds($epoch).UtcDateTime, [System.DateTimeKind]::Utc)

    # --- Build ------------------------------------------------------------------

    Write-Host "==> building the stagesync-desktop binary (profile: $Profile)"
    if ($Profile -eq 'release') {
        $buildExit = Invoke-NativeCommand $cargo 'build' '--release' '--locked' '--bin' 'stagesync-desktop'
    }
    else {
        $buildExit = Invoke-NativeCommand $cargo 'build' '--profile' $Profile '--locked' '--bin' 'stagesync-desktop'
    }
    if ($buildExit -ne 0) {
        throw "cargo build failed with exit code $buildExit"
    }

    $binaryName = if ($env:OS -eq 'Windows_NT') { 'stagesync-desktop.exe' } else { 'stagesync-desktop' }
    # Cargo's own target-directory override is honoured, so a build can share
    # a prepared dependency cache instead of compiling every crate again.
    $targetDir = if ($env:CARGO_TARGET_DIR) {
        if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
            $env:CARGO_TARGET_DIR
        }
        else {
            Join-Path $repoRoot $env:CARGO_TARGET_DIR
        }
    }
    else {
        Join-Path $repoRoot 'target'
    }
    $builtBinary = Join-Path $targetDir (Join-Path $Profile $binaryName)
    if (-not (Test-Path -LiteralPath $builtBinary)) {
        throw "the built binary was not found at '$builtBinary'"
    }

    # --- The notices bundle -----------------------------------------------------
    #
    # The notices are generated from Cargo.lock, so they always describe the
    # dependency graph that produced this binary. A missing Python is a hard
    # failure: shipping a binary without its notices is worse than shipping
    # no binary at all.

    $python = Resolve-PythonCommand
    if (-not $python) {
        throw ("Python 3 was not found, and the licence and notice bundle cannot be " +
            "generated without it. Install Python 3 (https://www.python.org/downloads/windows/) " +
            "or run this script from an environment that has it.")
    }

    $noticesSource = Join-Path $repoRoot (Join-Path 'packaging' 'notices')
    $generator = Join-Path $noticesSource 'generate_notices.py'
    if (-not (Test-Path -LiteralPath $generator)) {
        throw "the notices generator was not found at '$generator'"
    }

    $noticesStage = Join-Path $outputPath 'notices-stage'
    if (Test-Path -LiteralPath $noticesStage) {
        Remove-Item -LiteralPath $noticesStage -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $noticesStage | Out-Null

    Write-Host '==> generating the licence and notice bundle'
    $generatorExit = Invoke-NativeCommand $python $generator `
        '--cargo-lock' (Join-Path $repoRoot 'Cargo.lock') `
        '--licenses' (Join-Path $noticesSource 'licenses.json') `
        '--cargo-toml' (Join-Path $repoRoot 'Cargo.toml') `
        '--target' 'x86_64-pc-windows-msvc' `
        '--output' $noticesStage
    if ($generatorExit -ne 0) {
        throw "the notices generator failed with exit code $generatorExit"
    }

    $noticesDir = Join-Path $portableDir 'notices'
    New-Item -ItemType Directory -Force -Path $noticesDir | Out-Null
    foreach ($name in @('LICENSE-MIT', 'LICENSE-APACHE', 'LICENSE-MPL-2.0')) {
        $source = Join-Path $noticesSource $name
        if (-not (Test-Path -LiteralPath $source)) {
            throw "the licence text '$name' was not found in '$noticesSource'"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $noticesDir $name) -Force
    }
    foreach ($name in @('THIRD_PARTY_NOTICES.md', 'SOURCE-AVAILABILITY.md')) {
        $source = Join-Path $noticesStage $name
        if (-not (Test-Path -LiteralPath $source)) {
            throw "the generated notice '$name' was not produced by the generator"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $noticesDir $name) -Force
    }
    Write-Host "==> the notices bundle is at '$noticesDir'"

    # The staging directory has done its work; it is not part of the release.
    Remove-Item -LiteralPath $noticesStage -Recurse -Force

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
  notices/      the licence and third-party notice bundle

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

The licence and notice bundle
------------------------------
The notices/ folder holds the licence texts and the third-party notices for
this build: the licence of StageSync itself (MIT or Apache-2.0, your choice),
the licences of every dependency in Cargo.lock, and the Mozilla Public
License 2.0 that covers the symphonia audio decoder. symphonia is linked into
the binary, so the MPL requires that its source be available; the
notices/SOURCE-AVAILABILITY.md file says exactly where to get it.

What StageSync does with your files
-----------------------------------
StageSync never modifies a recording it is given. It reads the reference and
the source recordings you point it at, and writes the drift-corrected audio to
a directory you choose.
"@
    # Written without a byte order mark: the file is read by people, and a BOM
    # in a plain text file only makes the first line look odd.
    [System.IO.File]::WriteAllText($readmePath, $readme, (New-Object System.Text.UTF8Encoding($false)))

    # Every staged file gets the same modification time, so that the archive
    # and the installer carry it too.
    Get-ChildItem -LiteralPath $portableDir -Recurse -File |
        ForEach-Object { Set-FixedTimestamp -Path $_.FullName -Timestamp $timestamp }

    # --- The portable ZIP -------------------------------------------------------

    $zipPath = Join-Path $outputPath "stagesync-$version-windows-x64-portable.zip"
    Write-Host '==> zipping the portable folder'
    New-DeterministicZip -SourceDir $portableDir -ZipPath $zipPath -RootName 'portable' -Timestamp $timestamp

    # --- The installer ----------------------------------------------------------

    $setupPath = Join-Path $outputPath "stagesync-$version-windows-x64-setup.exe"
    $nsiPath = Join-Path $repoRoot (Join-Path 'packaging' (Join-Path 'windows' 'installer.nsi'))
    if (-not (Test-Path -LiteralPath $nsiPath)) {
        throw "the NSIS script was not found at '$nsiPath'"
    }

    $makensis = Get-Command 'makensis' -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
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
        # NSIS resolves `\` inside a string as an escape sequence: `\n` is a
        # newline and `\r` a carriage return. The paths handed to makensis are
        # therefore written with forward slashes, which NSIS accepts on
        # Windows and which cannot be misread.
        $nsisExe = ($builtBinary -replace '\\', '/')
        $nsisZip = ($zipPath -replace '\\', '/')
        $nsisNotices = ($noticesDir -replace '\\', '/')
        $nsisInstallDir = ($installDir -replace '\\', '/')
        # makensis writes its output file into the current directory, so the
        # script is run from the output directory.
        Push-Location $outputPath
        try {
            $nsisExit = Invoke-NativeCommand $makensis `
                "/DSTAGESYNC_VERSION=$version" `
                "/DSTAGESYNC_VERSION_NUM=$versionNumeric" `
                "/DSTAGESYNC_INSTALL_DIR=$nsisInstallDir" `
                "/DSTAGESYNC_EXE=$nsisExe" `
                "/DSTAGESYNC_PORTABLE_ZIP=$nsisZip" `
                "/DSTAGESYNC_NOTICES_DIR=$nsisNotices" `
                $nsiPath
            if ($nsisExit -ne 0) {
                throw "makensis failed with exit code $nsisExit"
            }
        }
        finally {
            Pop-Location
        }
        if (-not (Test-Path -LiteralPath $setupPath)) {
            throw "makensis reported success but '$setupPath' was not written"
        }
    }

    # --- The checksum manifest --------------------------------------------------
    #
    # SHA-256 of every artefact, in the `sha256sum` format, so that the
    # release can publish one manifest that anybody can check with
    # `sha256sum -c`.

    $manifestPath = Join-Path $outputPath 'SHA256SUMS.txt'
    $manifestLines = @()
    foreach ($artifact in @($zipPath, $setupPath)) {
        if (Test-Path -LiteralPath $artifact) {
            $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
            $manifestLines += "$hash  $([System.IO.Path]::GetFileName($artifact))"
        }
    }
    [System.IO.File]::WriteAllText(
        $manifestPath,
        (($manifestLines -join "`n") + "`n"),
        (New-Object System.Text.UTF8Encoding($false)))

    # --- What was written -------------------------------------------------------

    Write-Host ''
    Write-Host 'StageSync Windows packaging finished.'
    Write-Host "version           : $version"
    Write-Host "profile           : $Profile"
    Write-Host "portable binary   : $([System.IO.Path]::GetFullPath($portableBinary))"
    Write-Host "portable readme   : $([System.IO.Path]::GetFullPath($readmePath))"
    Write-Host "notices bundle    : $([System.IO.Path]::GetFullPath($noticesDir))"
    Write-Host "portable zip      : $([System.IO.Path]::GetFullPath($zipPath))"
    if (Test-Path -LiteralPath $setupPath) {
        Write-Host "installer         : $([System.IO.Path]::GetFullPath($setupPath))"
    }
    else {
        Write-Host "installer         : (not produced - NSIS was not available)"
    }
    Write-Host "checksum manifest : $([System.IO.Path]::GetFullPath($manifestPath))"
    Write-Host "output directory  : $outputPath"
}
finally {
    Pop-Location
}
