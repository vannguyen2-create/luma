# Install or update Luma on Windows
# Usage: irm https://raw.githubusercontent.com/nghyane/luma/master/install.ps1 | iex
$ErrorActionPreference = 'Stop'

$Repo = 'nghyane/luma'
$InstallDir = if ($env:LUMA_INSTALL_DIR) { $env:LUMA_INSTALL_DIR } else { "$env:USERPROFILE\.local\bin" }
$Target = 'x86_64-pc-windows-msvc'

# Find latest release
if ($env:LUMA_VERSION) {
    $Tag = $env:LUMA_VERSION
} else {
    $releases = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases?per_page=1"
    $Tag = $releases[0].tag_name
}

if (-not $Tag) {
    Write-Error 'Failed to detect latest version'
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Tag/luma-$Target.zip"

Write-Host "Installing luma $Tag ($Target)"
Write-Host "  from: $Url"
Write-Host "  to:   $InstallDir\luma.exe"

# Download and extract
$Tmp = New-TemporaryFile | ForEach-Object { Remove-Item $_; New-Item -ItemType Directory -Path $_ }
try {
    Invoke-WebRequest -Uri $Url -OutFile "$Tmp\luma.zip"
    Expand-Archive -Path "$Tmp\luma.zip" -DestinationPath $Tmp -Force

    # Install
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Move-Item -Path "$Tmp\luma.exe" -Destination "$InstallDir\luma.exe" -Force

    Write-Host "Installed luma $Tag"
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

# Add to PATH if missing
if ($env:PATH -notlike "*$InstallDir*") {
    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable('PATH', "$InstallDir;$UserPath", 'User')
        Write-Host "Added $InstallDir to user PATH"
    }
    $env:PATH = "$InstallDir;$env:PATH"
    Write-Host 'Restart terminal or run:  $env:PATH = "$InstallDir;$env:PATH"'
}
