$ErrorActionPreference = 'Stop'
if (-not $env:GITHUB_WORKSPACE) { throw 'GITHUB_WORKSPACE is required' }
if (-not $env:GITHUB_ENV) { throw 'GITHUB_ENV is required' }

$root = Join-Path $env:GITHUB_WORKSPACE 'third_party/ndi-sdk-6'
$hdr = Join-Path $root 'include/Processing.NDI.Lib.h'
$lib = Join-Path $root 'lib/x64/Processing.NDI.Lib.x64.lib'
$dll = Join-Path $root 'lib/x64/Processing.NDI.Lib.x64.dll'
if (-not (Test-Path -LiteralPath $hdr) -or -not (Test-Path -LiteralPath $lib) -or -not (Test-Path -LiteralPath $dll)) {
    Write-Host "::error::Vendored NDI SDK missing. Expected $hdr, $lib, and $dll. Run scripts/vendor-ndi-sdk-from-local.ps1 (see third_party/ndi-sdk-6/NOTICE)."
    exit 1
}

$resolved = (Resolve-Path -LiteralPath $root).Path
Add-Content -Path $env:GITHUB_ENV -Value "NDI_SDK_DIR=$resolved"
Write-Host "NDI_SDK_DIR=$resolved"
