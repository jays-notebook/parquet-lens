#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the Windows installer for parquet-lens.

.DESCRIPTION
    Produces a release Tauri bundle (NSIS installer by default). The script:

      1. Locates and imports the MSVC build environment (vcvars64.bat) so that
         `cargo`/`tauri build` can link. On this machine a plain shell lacks the
         MSVC linker on PATH, so this step is mandatory.
      2. Ensures frontend dependencies are installed. `tauri build` invokes the
         frontend build and uses the Tauri CLI, both of which live in
         node_modules; a missing tree would fail the build immediately.
      3. Runs `npm run tauri build -- --bundles <target>` from the project root.
      4. Retries on transient file-lock failures (os error 32). The corporate
         Bitdefender real-time scanner intermittently locks freshly written
         build artifacts; incremental compilation persists across runs, so a
         retry resumes and typically succeeds within a few attempts.
      5. Reports the path of each produced installer.

.PARAMETER Bundles
    Tauri bundle target(s) to produce. Default: "nsis". Accepts the same values
    as `tauri build --bundles` (e.g. "nsis", "msi", "nsis,msi").

.PARAMETER MaxRetries
    Maximum number of build attempts before giving up. Default: 4.

.EXAMPLE
    .\scripts\build-installer.ps1
    Builds the NSIS installer.

.EXAMPLE
    .\scripts\build-installer.ps1 -Bundles msi -MaxRetries 6
    Builds the MSI installer, allowing up to 6 attempts.
#>
[CmdletBinding()]
param(
    [string]$Bundles = "nsis",
    [int]$MaxRetries = 4
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Project root is the parent of this script's directory.
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Find-VcVars {
    # Prefer vswhere (ships with VS Installer) so the path is not hardcoded.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $installPath = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -property installationPath 2>$null
        if ($installPath) {
            $candidate = Join-Path $installPath "VC\Auxiliary\Build\vcvars64.bat"
            if (Test-Path $candidate) { return $candidate }
        }
    }

    # Fallback: the known install location on this dev machine.
    $fallback = "D:\Program Files\Microsoft\Visual Studio\VC\Auxiliary\Build\vcvars64.bat"
    if (Test-Path $fallback) { return $fallback }

    throw "Could not locate vcvars64.bat. Install the VS C++ Build Tools or adjust the fallback path in build-installer.ps1."
}

function Import-VcVarsEnv {
    param([string]$VcVarsPath)

    Write-Host "Importing MSVC environment from: $VcVarsPath" -ForegroundColor Cyan
    # Run vcvars in cmd, then dump the resulting environment and apply it here.
    $output = cmd /c "`"$VcVarsPath`" >nul 2>&1 && set"
    foreach ($line in $output) {
        if ($line -match '^([^=]+)=(.*)$') {
            Set-Item -Path "env:$($matches[1])" -Value $matches[2]
        }
    }
}

# --- Build ---------------------------------------------------------------

$vcvars = Find-VcVars
Import-VcVarsEnv -VcVarsPath $vcvars

Push-Location $ProjectRoot
try {
    # Ensure frontend dependencies are present. `tauri build` runs the frontend
    # build (beforeBuildCommand) and relies on the Tauri CLI, both of which live
    # in node_modules; a missing tree fails the build on the first line. Prefer
    # `npm ci` for a clean, lockfile-exact install when a lockfile is available.
    if (-not (Test-Path (Join-Path $ProjectRoot "node_modules"))) {
        Write-Host "node_modules not found - installing dependencies..." -ForegroundColor Cyan
        if (Test-Path (Join-Path $ProjectRoot "package-lock.json")) {
            & npm ci
        } else {
            & npm install
        }
        if ($LASTEXITCODE -ne 0) {
            throw "Dependency installation failed (exit $LASTEXITCODE)."
        }
    }

    $attempt = 0
    $succeeded = $false

    while ($attempt -lt $MaxRetries) {
        $attempt++
        Write-Host "=== Build attempt $attempt of $MaxRetries (bundles: $Bundles) ===" -ForegroundColor Yellow

        # npm exit code lands in $LASTEXITCODE; do not let a non-zero exit throw.
        & npm run tauri build -- --bundles $Bundles
        if ($LASTEXITCODE -eq 0) {
            $succeeded = $true
            break
        }

        Write-Host "Build failed (exit $LASTEXITCODE) - likely an AV file lock. Retrying..." -ForegroundColor Red
    }

    if (-not $succeeded) {
        throw "Build failed after $attempt attempt(s)."
    }

    Write-Host "=== BUILD SUCCEEDED on attempt $attempt ===" -ForegroundColor Green

    # Report the produced installer(s).
    $bundleRoot = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
    $installers = Get-ChildItem -Path $bundleRoot -Recurse -ErrorAction SilentlyContinue `
        -Include *.exe, *.msi | Sort-Object LastWriteTime -Descending
    if ($installers) {
        Write-Host "`nInstaller(s):" -ForegroundColor Green
        $installers | ForEach-Object { Write-Host "  $($_.FullName)" }
    } else {
        Write-Host "Build reported success but no installer was found under $bundleRoot." -ForegroundColor Yellow
    }
}
finally {
    Pop-Location
}
