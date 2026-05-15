#!/usr/bin/env pwsh
# Populates third_party/ndi-sdk-6 from local NDI v6 installers (layout expected by grafton-ndi as NDI_SDK_DIR).
# Default source: C:\Users\Flowing\Downloads\ndi_sdk  (-SourceDir to override).
# Needs: "NDI 6 SDK.exe", Install_NDI_SDK_v6_Linux.tar.gz, Install_NDI_SDK_v6_Apple.pkg
# Linux: WSL + yes|sh installer. macOS: tar on .pkg + inner Payload. Windows: silent Inno install.
param(
    [string] $SourceDir = "C:\Users\Flowing\Downloads\ndi_sdk",
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'

$out = Join-Path $RepoRoot 'third_party/ndi-sdk-6'
$winExe = Join-Path $SourceDir 'NDI 6 SDK.exe'
$linuxTar = Join-Path $SourceDir 'Install_NDI_SDK_v6_Linux.tar.gz'
$macPkg = Join-Path $SourceDir 'Install_NDI_SDK_v6_Apple.pkg'

foreach ($p in @($winExe, $linuxTar, $macPkg)) {
    if (-not (Test-Path -LiteralPath $p)) {
        throw "Missing required file: $p"
    }
}

$includeOut = Join-Path $out 'include'
$libWin = Join-Path $out 'lib/x64'
$libLnx = Join-Path $out 'lib/x86_64-linux-gnu'
$libMac = Join-Path $out 'lib/macOS'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $libWin, $libLnx, $libMac | Out-Null

# --- Windows ---
$stageWin = Join-Path $env:TEMP 'momaku-ndi-win-vendor'
Remove-Item -Recurse -Force $stageWin -ErrorAction SilentlyContinue
Write-Host 'Running Windows NDI SDK installer (silent)...'
$proc = Start-Process -FilePath $winExe -ArgumentList @(
    '/VERYSILENT', '/SP-', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOCANCEL', "/DIR=$stageWin"
) -PassThru -Wait
if (-not (Test-Path (Join-Path $stageWin 'Include/Processing.NDI.Lib.h'))) {
    throw "Windows NDI SDK not found under $stageWin (installer exit $($proc.ExitCode))"
}
New-Item -ItemType Directory -Force -Path $includeOut | Out-Null
Copy-Item -Recurse -Force (Join-Path $stageWin 'Include/*') $includeOut
Copy-Item -Force (Join-Path $stageWin 'Lib/x64/Processing.NDI.Lib.x64.lib') $libWin
$ndiDll = Join-Path $stageWin 'Lib/x64/Processing.NDI.Lib.x64.dll'
if (-not (Test-Path -LiteralPath $ndiDll)) {
    throw "Windows NDI runtime DLL missing under Lib/x64: $ndiDll"
}
Copy-Item -Force $ndiDll $libWin

# --- macOS ---
$stagePkg = Join-Path $env:TEMP 'momaku-ndi-pkg'
Remove-Item -Recurse -Force $stagePkg -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stagePkg | Out-Null
Push-Location $stagePkg
try {
    tar -xf $macPkg
    $payload = Join-Path $stagePkg 'NDI_SDK_Component.pkg/Payload'
    if (-not (Test-Path -LiteralPath $payload)) {
        throw "Payload not found after extracting $macPkg"
    }
    $stageMac = Join-Path $env:TEMP 'momaku-ndi-mac-tree'
    Remove-Item -Recurse -Force $stageMac -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $stageMac | Out-Null
    tar -xf $payload -C $stageMac
    $dy = Join-Path $stageMac 'NDI SDK for Apple/lib/macOS/libndi.dylib'
    if (-not (Test-Path -LiteralPath $dy)) { throw "Missing $dy" }
    Copy-Item -Force $dy (Join-Path $libMac 'libndi.dylib')
}
finally {
    Pop-Location
}

# --- Linux (WSL) ---
function ConvertTo-WslPath([string] $WindowsPath) {
    $p = $WindowsPath.TrimEnd('\')
    if ($p -notmatch '^([A-Za-z]):') { throw "Not a drive path: $WindowsPath" }
    $d = $Matches[1].ToLowerInvariant()
    $rest = $p.Substring(2).TrimStart('\').Replace('\', '/')
    "/mnt/$d/$rest"
}

$srcWsl = ConvertTo-WslPath $SourceDir
$dstWsl = ConvertTo-WslPath $out
$wslCmd = @"
set -eux
cd /tmp
rm -rf momaku-ndi-lnx && mkdir momaku-ndi-lnx && cd momaku-ndi-lnx
cp '$srcWsl/Install_NDI_SDK_v6_Linux.tar.gz' .
tar -xzf Install_NDI_SDK_v6_Linux.tar.gz
yes | PAGER=cat sh ./Install_NDI_SDK_v6_Linux.sh
install -d '$dstWsl/lib/x86_64-linux-gnu'
cp -a "NDI SDK for Linux/lib/x86_64-linux-gnu"/libndi.so.*.*.* '$dstWsl/lib/x86_64-linux-gnu/'
"@
wsl -e bash -c $wslCmd

Write-Host "OK: vendored -> $out"
$sum = (Get-ChildItem $out -Recurse -File | Measure-Object -Property Length -Sum).Sum
Write-Host "Total file bytes: $sum"
