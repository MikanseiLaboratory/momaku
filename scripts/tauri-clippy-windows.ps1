#!/usr/bin/env pwsh
# `npm run tauri:dev:win` と同じ Windows 環境（scripts/_tauri-windows-env.ps1）で clippy を実行します。
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. "$PSScriptRoot\_tauri-windows-env.ps1"
cargo clippy -p momaku --all-targets --all-features --locked -- -D warnings
