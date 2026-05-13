#!/usr/bin/env pwsh
# Mirrors `.github/workflows/release.yml` (Windows job): MSVC+LLVM env, NDI + signing checks, `npm ci`, then
# `tauri build` with updater artifacts (`bundle-with-updater.conf.json`). Run from repo root via `npm run tauri:release:win`.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"

$ndi = Join-Path ${env:ProgramFiles} 'NDI\NDI 6 SDK'
$vendored = Join-Path $PWD 'third_party\ndi-sdk-6'
$vendoredOk = (Test-Path (Join-Path $vendored 'include\Processing.NDI.Lib.h')) -and (Test-Path (Join-Path $vendored 'lib\x64\Processing.NDI.Lib.x64.lib'))
if (-not (Test-Path $ndi)) {
    if ($vendoredOk) {
        $env:NDI_SDK_DIR = (Resolve-Path -LiteralPath $vendored).Path
        Write-Host "NDI_SDK_DIR=$env:NDI_SDK_DIR (third_party/ndi-sdk-6; Program Files SDK not installed)"
    } else {
        throw "NDI 6 SDK not found at $ndi and vendored third_party/ndi-sdk-6 is incomplete. Install from https://ndi.video/type/developer/ or sync third_party/ndi-sdk-6 from the repo."
    }
}

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

npm ci
npm run tauri -- build --verbose --config src-tauri/bundle-with-updater.conf.json
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
