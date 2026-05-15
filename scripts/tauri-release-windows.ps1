#!/usr/bin/env pwsh
# Mirrors `.github/workflows/release.yml` (Windows job): MSVC+LLVM env, NDI + signing checks, `npm ci`, then
# `tauri build` with updater artifacts (`bundle-with-updater.conf.json`). Run from repo root via `npm run tauri:release:win`.
param(
    # Use when npm ci hits EPERM on cli.win32-x64-msvc.node (file locked); requires a good node_modules already.
    [switch] $SkipNpmCi
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"

$vendored = Join-Path $PWD 'third_party\ndi-sdk-6'
$hdr = Join-Path $vendored 'include\Processing.NDI.Lib.h'
$lib = Join-Path $vendored 'lib\x64\Processing.NDI.Lib.x64.lib'
$dll = Join-Path $vendored 'lib\x64\Processing.NDI.Lib.x64.dll'
if (-not (Test-Path $hdr) -or -not (Test-Path $lib) -or -not (Test-Path $dll)) {
    throw "Vendored NDI SDK missing under $vendored (need .lib and .dll under lib\x64). Run scripts/vendor-ndi-sdk-from-local.ps1 or clone a revision that includes third_party/ndi-sdk-6."
}
$env:NDI_SDK_DIR = (Resolve-Path -LiteralPath $vendored).Path
Write-Host "NDI_SDK_DIR=$env:NDI_SDK_DIR"

if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY) -and [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PATH)) {
    $defaultKey = Join-Path $PWD 'src-tauri\momaku-signing.key'
    if (Test-Path $defaultKey) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = (Resolve-Path $defaultKey).Path
    }
}
if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY) -and [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PATH)) {
    throw 'Set TAURI_SIGNING_PRIVATE_KEY (CI) or TAURI_SIGNING_PRIVATE_KEY_PATH / place src-tauri/momaku-signing.key for local release builds.'
}

# Updater signing reads `TAURI_SIGNING_PRIVATE_KEY` (same as Actions secrets); load from path when only PATH is set.
if (-not [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PATH) -and [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
    $raw = Get-Content -LiteralPath $env:TAURI_SIGNING_PRIVATE_KEY_PATH -Raw
    if ($raw -match 'minisign encrypted' -and [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
        throw 'Encrypted signing key requires TAURI_SIGNING_PRIVATE_KEY_PASSWORD for non-interactive builds (set locally or as a GitHub Actions secret).'
    }
    $env:TAURI_SIGNING_PRIVATE_KEY = $raw
}

if ($SkipNpmCi) {
    $tauriBin = Join-Path $PWD 'node_modules\.bin\tauri.cmd'
    if (-not (Test-Path -LiteralPath $tauriBin)) {
        throw "SkipNpmCi was set but $tauriBin is missing. Run npm ci once with other apps (Cursor, tauri dev) closed, or run npm install."
    }
    Write-Host 'SkipNpmCi: skipping npm ci (using existing node_modules).'
} else {
    Write-Host 'Running npm ci… (if EPERM on cli.win32-x64-msvc.node: close this repo in other editors, stop `npm run dev` / `tauri dev`, then retry.)'
    npm ci
    if ($LASTEXITCODE -ne 0) {
        Write-Host @'

npm ci failed. On Windows EPERM unlink on @tauri-apps/cli native binary usually means the file is locked.
Try: close Cursor/VS Code for this folder, end Node/tauri processes, wait a few seconds, re-run.
Or re-run with: pwsh -File scripts/tauri-release-windows.ps1 -SkipNpmCi
(after a successful npm ci or npm install at least once).

'@
        exit $LASTEXITCODE
    }
}

npm exec -- tauri build --verbose --config src-tauri/bundle-with-updater.conf.json --config src-tauri/bundle-release-windows.json
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& "$PSScriptRoot\verify-windows-bundle-dlls.ps1" -RepoRoot $PWD
