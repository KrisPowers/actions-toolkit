# Installs the actions-toolkit CLI (backend + embedded UI in one binary).
#
#   irm https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.ps1 | iex
#
# Env overrides:
#   ACTIONS_TOOLKIT_VERSION     release tag to install, e.g. v0.1.0 (default: latest)
#   ACTIONS_TOOLKIT_INSTALL_DIR directory to install the binary into (default: %LOCALAPPDATA%\actions-toolkit\bin)

$ErrorActionPreference = "Stop"

$Repo = "KrisPowers/actions-toolkit"
$BinName = "actions-toolkit.exe"
$InstallDir = if ($env:ACTIONS_TOOLKIT_INSTALL_DIR) { $env:ACTIONS_TOOLKIT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "actions-toolkit\bin" }
$Version = if ($env:ACTIONS_TOOLKIT_VERSION) { $env:ACTIONS_TOOLKIT_VERSION } else { "latest" }

$arch = if ([System.Environment]::Is64BitOperatingSystem) { "x86_64" } else { $null }
if ($arch -ne "x86_64") {
  Write-Error "error: unsupported architecture (only windows/x86_64 has a prebuilt binary). Build from source instead: see README.md"
  exit 1
}

$asset = "actions-toolkit-windows-$arch"
if ($Version -eq "latest") {
  $url = "https://github.com/$Repo/releases/latest/download/$asset.zip"
} else {
  $url = "https://github.com/$Repo/releases/download/$Version/$asset.zip"
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
try {
  $zipPath = Join-Path $tmpDir "$asset.zip"

  Write-Host "Downloading actions-toolkit (windows-$arch, $Version)..."
  try {
    Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
  } catch {
    Write-Error "error: download failed from $url`nCheck available versions at https://github.com/$Repo/releases"
    exit 1
  }

  Expand-Archive -Path $zipPath -DestinationPath $tmpDir -Force

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item (Join-Path $tmpDir $BinName) (Join-Path $InstallDir $BinName) -Force
} finally {
  Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}

Write-Host "Installed actions-toolkit to $InstallDir\$BinName"

Write-Host "Initializing data directory..."
& (Join-Path $InstallDir $BinName) init

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (";$userPath;" -notlike "*;$InstallDir;*") {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
  $env:Path = "$env:Path;$InstallDir"
  Write-Host ""
  Write-Host "Added $InstallDir to your user PATH. Open a new terminal for it to take effect."
}

Write-Host ""
Write-Host "Run 'actions-toolkit start' (or 'actions-toolkit listen') to launch actions-toolkit."
