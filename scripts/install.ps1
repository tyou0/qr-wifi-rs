# Compatible with Windows PowerShell 5.1 and PowerShell 7.
[CmdletBinding()]
param(
    [string]$PrebuiltDirectory,
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\QR Wi-Fi RS'),
    [string]$ShortcutDirectory = [Environment]::GetFolderPath('Programs'),
    [switch]$NoPathUpdate
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Use scripts/install.sh on macOS or Linux.' }

function Invoke-Cargo {
    & cargo @args
    if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit code $LASTEXITCODE" }
}

if (-not $PrebuiltDirectory -and (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'platform.txt'))) {
    $PrebuiltDirectory = $PSScriptRoot
}
if ($PrebuiltDirectory) {
    $platform = (Get-Content -LiteralPath (Join-Path $PrebuiltDirectory 'platform.txt') -Raw).Trim()
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $architecture = $env:PROCESSOR_ARCHITEW6432 }
    if ($platform -ne 'windows-x86_64' -or $architecture -ne 'AMD64') {
        throw 'This archive requires x64 Windows. Build from source for other architectures.'
    }
    $binaries = Join-Path $PrebuiltDirectory 'bin'
} else {
    $repo = Split-Path -Parent $PSScriptRoot
    if (-not (Test-Path -LiteralPath (Join-Path $repo 'Cargo.toml'))) { throw 'Source checkout missing.' }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'Install Rust (MSVC), Visual Studio C++ Build Tools, and WebView2 first. See README.'
    }
    Push-Location $repo
    $previousTargetDirectory = $env:CARGO_TARGET_DIR
    try {
        $rustInfo = & rustc -vV
        if ($LASTEXITCODE -ne 0) { throw 'rustc failed.' }
        $target = ($rustInfo | Select-String '^host: (.+)$').Matches.Groups[1].Value
        if ($target -notlike '*-pc-windows-msvc') { throw 'The MSVC Rust toolchain is required.' }
        $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
        if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
            Invoke-Cargo install tauri-cli --locked --version 2.11.4
        }
        Invoke-Cargo build --release --locked --target $target -p qr-wifi-cli -p qr-wifi-tui -p qr-wifi-host
        Invoke-Cargo tauri build --ci --target $target --no-bundle -- --locked
        $binaries = Join-Path $env:CARGO_TARGET_DIR "$target\release"
    } finally {
        $env:CARGO_TARGET_DIR = $previousTargetDirectory
        Pop-Location
    }
}

# Check all artifacts before creating a partial installation.
$names = @('qr-wifi', 'qr-wifi-tui', 'qr-wifi-host', 'qr-wifi-gui')
foreach ($name in $names) {
    if (-not (Test-Path -LiteralPath (Join-Path $binaries "$name.exe") -PathType Leaf)) {
        throw "Missing executable: $name.exe"
    }
}
$bin = Join-Path ([IO.Path]::GetFullPath($InstallDirectory)) 'bin'
New-Item -ItemType Directory -Force -Path $bin, $ShortcutDirectory | Out-Null
foreach ($name in $names) {
    Copy-Item -LiteralPath (Join-Path $binaries "$name.exe") -Destination $bin -Force
}
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut((Join-Path $ShortcutDirectory 'QR Wi-Fi RS.lnk'))
$shortcut.TargetPath = Join-Path $bin 'qr-wifi-gui.exe'
$shortcut.WorkingDirectory = $bin
$shortcut.Save()

if (-not $NoPathUpdate) {
    [string]$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (@($userPath -split ';') -notcontains $bin) {
        [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ';' + $bin).TrimStart(';')), 'User')
    }
    $env:Path = "$bin;$env:Path"
}
Write-Host "Installed desktop, CLI, TUI, and native host to $InstallDirectory."
Write-Host 'Open QR Wi-Fi RS from Start. Reopen your terminal for qr-wifi and qr-wifi-tui.'
Write-Host 'Desktop runtime requires Microsoft Edge WebView2 (see README).'
