#!/usr/bin/env pwsh
# After a Windows release build, checks that momaku.exe sits next to NDI + ANGLE DLLs
# (same layout as NSIS install). Run from repo root; optional -RepoRoot.
param(
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'

$targetRoots = @(
    (Join-Path $RepoRoot 'target'),
    (Join-Path $RepoRoot 'src-tauri/target')
)

$exe = $null
foreach ($root in $targetRoots) {
    if (-not (Test-Path -LiteralPath $root)) { continue }
    $exe = Get-ChildItem -Path $root -Recurse -Filter 'momaku.exe' -File -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '[\\/]release[\\/]' -and $_.FullName -notmatch '[\\/]deps[\\/]' } |
        Select-Object -First 1
    if ($exe) { break }
}

if (-not $exe) {
    throw "verify-windows-bundle-dlls: momaku.exe not found under target/ or src-tauri/target/ (release, excluding deps)."
}

$dir = $exe.DirectoryName
$need = @(
    'Processing.NDI.Lib.x64.dll',
    'libEGL.dll',
    'libGLESv2.dll'
)

foreach ($name in $need) {
    $p = Join-Path $dir $name
    if (-not (Test-Path -LiteralPath $p)) {
        throw "verify-windows-bundle-dlls: missing $name next to $($exe.FullName) (expected $p)."
    }
}

Write-Host "OK: release directory has momaku.exe + NDI + ANGLE DLLs under $dir"
