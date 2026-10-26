# Installs the Actions Toolkit CLI, `atk`.
#
#   irm https://atk.krispowers.dev | iex
#
# Env overrides:
#   ATK_VERSION     release tag to install, e.g. v0.1.0 (default: latest)
#   ATK_INSTALL_DIR directory to install the binary into (default: %LOCALAPPDATA%\atk\bin)

$ErrorActionPreference = "Stop"

$Repo = "KrisPowers/actions-toolkit"
$BinName = "atk.exe"
$InstallDir = if ($env:ATK_INSTALL_DIR) { $env:ATK_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "atk\bin" }
$Version = if ($env:ATK_VERSION) { $env:ATK_VERSION } else { "latest" }

$arch = if ([System.Environment]::Is64BitOperatingSystem) { "x86_64" } else { $null }
if ($arch -ne "x86_64") {
  Write-Error "error: unsupported architecture (only windows/x86_64 has a prebuilt binary). Build from source instead: see README.md"
  exit 1
}

$asset = "atk-windows-$arch"
if ($Version -eq "latest") {
  $url = "https://github.com/$Repo/releases/latest/download/$asset.zip"
} else {
  $url = "https://github.com/$Repo/releases/download/$Version/$asset.zip"
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
try {
  $zipPath = Join-Path $tmpDir "$asset.zip"

  Write-Host "Downloading atk (windows-$arch, $Version)..."
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

Write-Host "Installed atk to $InstallDir\$BinName"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (";$userPath;" -notlike "*;$InstallDir;*") {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
  $env:Path = "$env:Path;$InstallDir"
  Write-Host ""
  Write-Host "Added $InstallDir to your user PATH. Open a new terminal for it to take effect."
}

Write-Host ""
Write-Host "ATK runs every job in Docker. Run 'atk doctor' to check your setup, then 'atk' in a project."
