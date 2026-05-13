#!/usr/bin/env pwsh
# Tauri本番ビルド（`tauri build`）。リポジトリルートで実行してください。
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"
npm run tauri -- build
