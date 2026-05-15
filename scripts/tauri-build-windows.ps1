#!/usr/bin/env pwsh
# Tauri本番ビルド（`tauri build`）。リポジトリルートで実行してください。
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"
npm run tauri -- build --config src-tauri/bundle-release-windows.json
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& "$PSScriptRoot\verify-windows-bundle-dlls.ps1" -RepoRoot $PWD
