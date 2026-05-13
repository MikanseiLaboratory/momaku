$ErrorActionPreference = 'Stop'
if (-not $env:GITHUB_WORKSPACE) { throw 'GITHUB_WORKSPACE is required' }
if (-not $env:GITHUB_ENV) { throw 'GITHUB_ENV is required' }

$root = Join-Path $env:GITHUB_WORKSPACE '.ndi'
$tmp = Join-Path $root 'tmp'
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Set-Location $tmp

$ua = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
$urls = @()
if (-not [string]::IsNullOrWhiteSpace($env:NDI_SDK_WINDOWS_URL)) {
    $urls += $env:NDI_SDK_WINDOWS_URL
}
$urls += @(
    'https://downloads.ndi.tv/SDK/NDI_SDK_Windows/Install_NDI_SDK_v6_Windows_Console.zip'
)

Write-Host '::group::Download NDI SDK for Windows'
$zipPath = Join-Path $tmp 'ndi-win.zip'
$ok = $false
foreach ($u in $urls) {
    Remove-Item -Force -ErrorAction SilentlyContinue $zipPath
    curl.exe -fL --retry 5 -A $ua -o $zipPath -- $u
    if ($LASTEXITCODE -eq 0 -and (Test-Path $zipPath) -and (Get-Item $zipPath).Length -gt 1000) {
        $ok = $true
        break
    }
}
if (-not $ok) {
    Write-Host '::error::Could not download NDI SDK for Windows. Host a zip of the SDK tree (include\, lib\x64\) and set repository secret NDI_SDK_WINDOWS_URL.'
    exit 1
}
Write-Host '::endgroup::'

Write-Host '::group::Extract NDI SDK for Windows'
$extracted = Join-Path $tmp 'extracted'
if (Test-Path $extracted) { Remove-Item -Recurse -Force $extracted }
Expand-Archive -Path $zipPath -DestinationPath $extracted -Force
$hdr = Get-ChildItem -Path (Join-Path $tmp 'extracted') -Recurse -Filter 'Processing.NDI.Lib.h' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $hdr) {
    Write-Host '::error::Processing.NDI.Lib.h not found in archive. Ensure NDI_SDK_WINDOWS_URL points to a full SDK layout.'
    exit 1
}
$sdk = (Resolve-Path (Join-Path $hdr.Directory.FullName '..')).Path
$dest = Join-Path $root 'ndisdk-pre'
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
Move-Item -Path $sdk -Destination $dest
Write-Host '::endgroup::'

Add-Content -Path $env:GITHUB_ENV -Value "NDI_SDK_DIR=$dest"
Write-Host "Configured NDI_SDK_DIR=$dest"
