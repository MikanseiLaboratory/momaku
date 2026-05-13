#!/usr/bin/env pwsh
# Tauri開発サーバ（`tauri dev`）。リポジトリルートで実行してください。
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"
npm run tauri -- dev
