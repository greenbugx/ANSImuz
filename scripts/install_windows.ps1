<#
.SYNOPSIS
    Automated Windows installation script for ANSImuz.
.DESCRIPTION
    Installs and verifies all required prerequisites:
    - mpv (audio playback engine)
    - yt-dlp (stream resolution)
    - Rust toolchain (cargo, rustc)
    - Python 3.10+ and .venv virtual environment
    - Python dependencies from requirements.txt
    - Compiles ANSImuz release binary
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$TotalSteps = 6

function Write-Step {
    param (
        [string]$Number,
        [string]$Title
    )
    $Percent = [int](($Number / $TotalSteps) * 100)
    Write-Progress -Activity "Installing ANSImuz" -Status "Step $Number of $TotalSteps: $Title" -PercentComplete $Percent
    Write-Host "`n[$Number/$TotalSteps] " -ForegroundColor Cyan -NoNewline
    Write-Host "$Title" -ForegroundColor White
}

function Write-Success {
    param ([string]$Message)
    Write-Host "  ✓ " -ForegroundColor Green -NoNewline
    Write-Host "$Message" -ForegroundColor Gray
}

function Write-Info {
    param ([string]$Message)
    Write-Host "  → " -ForegroundColor Yellow -NoNewline
    Write-Host "$Message" -ForegroundColor Gray
}

function Write-ErrorExit {
    param ([string]$Message)
    Write-Host "`n[ERROR] $Message" -ForegroundColor Red
    exit 1
}

# Resolve repository root
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..")
Set-Location -Path $RepoRoot

Write-Host @"

 ▄░░░▄ ░░░░▄ ▄░░░ ░░ ▄░░▄░░░▄ ░░ ░░ ░░░▄      ░░ ░░░░▄ ▄░░░ ░░░░░░  ▄░░░▄ ░░  ░░  ▄░░░  ░░░░▄
 ░░ ░░ ░░ ░░ ░░   ░░ ░░ ░░ ░░ ░░ ░░   ░░      ░░ ░░ ░░ ░░   ▀ ░░ ▀  ░░ ░░ ░░  ░░  ░░ ▀  ░░ ░░
 ▒▒▒▒▒ ▒▒ ▒▒ ▀▒▒▄ ▒▒ ▒▒ ▒▒ ▒▒ ▒▒ ▒▒ ▄▒▒▀      ▒▒ ▒▒ ▒▒ ▀▒▒▄   ▒▒    ▒▒▒▒▒ ▒▒  ▒▒  ▒▒▒   ▒▒▒▒▀
 ▓█ ▓█ ▓█ ▓█   ▓█ ▓█ ▓█ ▓█ ▓█ ▓█ ▓█ ▓█        ▓█ ▓█ ▓█   ▓█   ▓█    ▓█ ▓█ ▓█  ▓█  ▓█    ▓█ ▓█
 ▄▄ ▄▄ ▄▄ ▄▄   ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄        ▄▄ ▄▄ ▄▄   ▄▄   ▄▄    ▄▄ ▄▄ ▄▄  ▄▄  ▄▄    ▄▄ ▄▄
 ░░ ░░ ░░ ░░   ░░ ░░ ░░ ░░ ░░ ░░ ░░ ░░        ░░ ░░ ░░   ░░   ░░    ░░ ░░ ▓▓  ▓▓  ░░    ░░ ░░
 ▒▒ ▒▒ ▒▒ ▒▒   ▒▒ ▒▒ ▒▒    ▒▒ ▒▒ ▒▒ ▒▒        ▒▒ ▒▒ ▒▒   ▒▒   ▒▒    ▒▒ ▒▒ ▒▒  ▒▒  ▒▒ ▄  ▒▒ ▒▒
▄▓▓ ▓▓ ▓▓ ▓▓ ▓▓▓▀ ▓▓ ▓▓    ▓▓ ▀▓▓▓▀ ▀▓▓▓      ▓▓ ▓▓ ▓▓ ▓▓▓▀  ▄▓▓▄  ▄▓▓ ▓▓ ▀▓▓ ▀▓▓ ▀▓▓▓ ▄▓▓ ▓▓

"@ -ForegroundColor Green

Write-Host "ANSImuz Windows Automated Installation Script" -ForegroundColor White
Write-Host "Target repository: $RepoRoot`n" -ForegroundColor Gray

# -------------------------------------------------------------
# Check Windows Environment & Package Managers
# -------------------------------------------------------------
Write-Step "1" "Checking Windows environment & package managers..."

$HasWinget = [bool](Get-Command winget -ErrorAction SilentlyContinue)
$HasChoco = [bool](Get-Command choco -ErrorAction SilentlyContinue)
$HasScoop = [bool](Get-Command scoop -ErrorAction SilentlyContinue)

if ($HasWinget) {
    Write-Success "Found package manager: winget (Windows Package Manager)"
} elseif ($HasChoco) {
    Write-Success "Found package manager: Chocolatey"
} elseif ($HasScoop) {
    Write-Success "Found package manager: Scoop"
} else {
    Write-Info "No automated package manager found. Pre-installed tools will be checked."
}

# -------------------------------------------------------------
# Verify or install mpv and yt-dlp
# -------------------------------------------------------------
Write-Step "2" "Verifying media prerequisites (mpv and yt-dlp)..."

# MPV
$MpvCmd = Get-Command mpv -ErrorAction SilentlyContinue
if (-not $MpvCmd) {
    Write-Info "mpv was not found in PATH. Attempting automated install..."
    if ($HasWinget) {
        Write-Info "Installing mpv via winget..."
        winget install --id shinchiro.mpv -e --accept-package-agreements --accept-source-agreements --silent
    } elseif ($HasChoco) {
        choco install mpv -y
    } elseif ($HasScoop) {
        scoop install mpv
    } else {
        Write-ErrorExit "mpv is required for audio playback. Please install mpv from https://mpv.io/ and add it to your PATH."
    }
    # Refresh PATH environment variable
    $env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
} else {
    Write-Success "mpv is already installed: $($MpvCmd.Source)"
}

# yt-dlp
$YtdlpCmd = Get-Command yt-dlp -ErrorAction SilentlyContinue
if (-not $YtdlpCmd) {
    Write-Info "yt-dlp was not found in PATH. Attempting automated install..."
    if ($HasWinget) {
        Write-Info "Installing yt-dlp via winget..."
        winget install --id yt-dlp.yt-dlp -e --accept-package-agreements --accept-source-agreements --silent
    } elseif ($HasChoco) {
        choco install yt-dlp -y
    } elseif ($HasScoop) {
        scoop install yt-dlp
    } else {
        Write-Info "yt-dlp will be installed in the Python virtual environment as fallback."
    }
    $env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
} else {
    Write-Success "yt-dlp is already installed: $($YtdlpCmd.Source)"
}

# -------------------------------------------------------------
# Verify Rust toolchain
# -------------------------------------------------------------
Write-Step "3" "Checking Rust toolchain (cargo and rustc)..."

# Include cargo in user profile if present
$CargoHome = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $CargoHome) -and ($env:PATH -notlike "*$CargoHome*")) {
    $env:PATH = "$CargoHome;$env:PATH"
}

$CargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
$RustcCmd = Get-Command rustc -ErrorAction SilentlyContinue

if (-not $CargoCmd -or -not $RustcCmd) {
    Write-Info "Rust toolchain not found. Attempting installation via winget..."
    if ($HasWinget) {
        winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
        $env:PATH = "$CargoHome;" + [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
    } else {
        Write-ErrorExit "Rust is required to build ANSImuz. Please install Rust from https://rustup.rs/ or https://win.rustup.rs/ and rerun this script."
    }
}

$RustVer = (& rustc --version)
$CargoVer = (& cargo --version)
Write-Success "Rust toolchain ready: $RustVer"

# -------------------------------------------------------------
# Setup Python virtual environment (.venv)
# -------------------------------------------------------------
Write-Step "4" "Setting up Python virtual environment (.venv)..."

# Locate python
$PythonCmd = Get-Command python -ErrorAction SilentlyContinue
if (-not $PythonCmd) {
    $PythonCmd = Get-Command py -ErrorAction SilentlyContinue
}

if (-not $PythonCmd) {
    if ($HasWinget) {
        Write-Info "Python not found. Installing Python 3.11 via winget..."
        winget install --id Python.Python.3.11 -e --accept-package-agreements --accept-source-agreements --silent
        $env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
        $PythonCmd = Get-Command python -ErrorAction SilentlyContinue
    }
}

if (-not $PythonCmd) {
    Write-ErrorExit "Python 3.10+ is required. Please install Python from https://www.python.org/ or Microsoft Store."
}

$PyVer = & $PythonCmd.Source --version
Write-Success "System Python found: $PyVer"

$VenvDir = Join-Path $RepoRoot ".venv"
$VenvPython = Join-Path $VenvDir "Scripts\python.exe"

if (-not (Test-Path $VenvPython)) {
    Write-Info "Creating virtual environment in $VenvDir..."
    & $PythonCmd.Source -m venv "$VenvDir"
}

if (-not (Test-Path $VenvPython)) {
    Write-ErrorExit "Failed to create Python virtual environment at $VenvDir."
}

Write-Success "Virtual environment ready at $VenvDir"

# -------------------------------------------------------------
# Install Python requirements
# -------------------------------------------------------------
Write-Step "5" "Installing Python dependencies from requirements.txt..."

& $VenvPython -m pip install --upgrade pip --quiet
$ReqFile = Join-Path $RepoRoot "requirements.txt"
& $VenvPython -m pip install -r "$ReqFile" --quiet

# Also ensure yt-dlp is in virtualenv if not on PATH
if (-not (Get-Command yt-dlp -ErrorAction SilentlyContinue)) {
    & $VenvPython -m pip install yt-dlp --quiet
}

Write-Success "Python packages installed (ytmusicapi, spotifyscraper, pillow, requests)."

# -------------------------------------------------------------
# Compile ANSImuz release binary
# -------------------------------------------------------------
Write-Step "6" "Building ANSImuz release binary with Cargo..."

& cargo build --release

$BinPath = Join-Path $RepoRoot "target\release\ansimuz.exe"
if (-not (Test-Path $BinPath)) {
    Write-ErrorExit "Compiled executable was not found at $BinPath."
}

Write-Success "Executable compiled successfully: $BinPath"
Write-Progress -Activity "Installing ANSImuz" -Completed

# -------------------------------------------------------------
# FINISHED
# -------------------------------------------------------------
Write-Host "`n======================================================" -ForegroundColor Green
Write-Host "  Installation Complete! ANSImuz is ready to run.     " -ForegroundColor Green
Write-Host "======================================================" -ForegroundColor Green
Write-Host "`nTo launch ANSImuz in PowerShell:" -ForegroundColor White
Write-Host "  .\.venv\Scripts\Activate.ps1" -ForegroundColor Cyan
Write-Host "  cargo run --release" -ForegroundColor Cyan
Write-Host "or launch the executable directly:" -ForegroundColor White
Write-Host "  .\target\release\ansimuz.exe`n" -ForegroundColor Cyan
