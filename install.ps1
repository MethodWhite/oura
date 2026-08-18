#!/usr/bin/env pwsh
#requires -Version 5.1

param(
  [string]$Version = "latest",
  [string]$InstallDir = "${env:ProgramFiles}\Oura",
  [string]$ConfigDir = "${env:USERPROFILE}\.config\oura"
)

$Repo = "MethodWhite/oura"
$Green  = "Green"
$Yellow = "Yellow"
$Red    = "Red"
$Cyan   = "Cyan"

function Log  { Write-Host "[✓] $args" -ForegroundColor $Green }
function Warn { Write-Host "[!] $args" -ForegroundColor $Yellow }
function Err  { Write-Host "[✗] $args" -ForegroundColor $Red; exit 1 }
function Info { Write-Host "[i] $args" -ForegroundColor $Cyan }

function Detect-Arch {
  $arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
  $env = [Environment]::GetEnvironmentVariable("PROCESSOR_IDENTIFIER")
  if ($env -match "ARM|AArch64") { $arch = "aarch64" }
  return $arch
}

function Get-ReleaseUrl {
  param([string]$OsArch)
  if ($Version -eq "latest") {
    return "https://github.com/$Repo/releases/latest/download/oura-${OsArch}.exe"
  }
  return "https://github.com/$Repo/releases/download/$Version/oura-${OsArch}.exe"
}

function Main {
  Write-Host ""
  Write-Host "╔══════════════════════════════════════╗" -ForegroundColor $Cyan
  Write-Host "║      Oura Installer — 0XFFRice       ║" -ForegroundColor $Cyan
  Write-Host "╚══════════════════════════════════════╝" -ForegroundColor $Cyan
  Write-Host ""

  $elevated = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  if (-not $elevated) {
    Warn "Not running as Administrator. Some installs may fail."
    Warn "Restart with: Start-Process pwsh -Verb RunAs -ArgumentList '-File install.ps1'"
  }

  $arch = Detect-Arch
  $osArch = "${arch}-pc-windows-msvc"
  $url = Get-ReleaseUrl -OsArch $osArch

  Info "Detected: $osArch"
  Info "Download: $url"

  $tmpdir = "$env:TEMP\oura-install"
  New-Item -ItemType Directory -Force -Path $tmpdir | Out-Null
  $outFile = "$tmpdir\oura.exe"

  try {
    Invoke-WebRequest -Uri $url -OutFile $outFile -UseBasicParsing -ErrorAction Stop
  } catch {
    Err "Download failed: $_"
  }

  try {
    $version = & $outFile --version 2>&1
    if (-not $?) { Err "Binary validation failed" }
  } catch {
    Err "Binary validation failed: $_"
  }

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item $outFile "$InstallDir\oura.exe" -Force
  Log "Installed: $InstallDir\oura.exe"

  $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
  if ($userPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$userPath", "User")
    Log "Added to PATH (user scope)"
  }

  New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
  $configFile = "$ConfigDir\config.toml"
  if (-not (Test-Path $configFile)) {
    @"
[loop_engine]
max_iterations = 20
convergence_threshold = 90.0
feedback_sources = ["test", "lint"]

[github]
enabled = true
default_owner = "MethodWhite"
default_repo = "my-project"
auto_commit = true
auto_pr = true

# [synapsis]  # Optional: uncomment for Synapsis integration (separate project)
# enabled = true
# endpoint = "http://localhost:7438"
"@ | Out-File -FilePath $configFile -Encoding utf8
    Log "Config created: $configFile"
  } else {
    Warn "Config exists at $configFile — skipping"
  }

  ""
  Log "Oura installed successfully!"
  & "$InstallDir\oura.exe" version 2>$null
  ""
}

Main
