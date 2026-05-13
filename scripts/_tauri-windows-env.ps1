# Shared setup for Tauri on Windows (mozangle / bindgen + MSVC).
# Dot-source from tauri-dev-windows.ps1 / tauri-build-windows.ps1
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) {
    throw "vswhere.exeが見つかりません。Visual Studio Installerを入れてください: $vswhere"
}

$installPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $installPath) {
    throw 'Visual Studio 2022（C++ デスクトップ開発）が見つかりません。'
}

$devShell = Join-Path $installPath 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll'
if (-not (Test-Path $devShell)) {
    throw "DevShellが見つかりません: $devShell"
}

Import-Module $devShell
Enter-VsDevShell -VsInstallPath $installPath -SkipAutomaticLocation -Arch amd64 -HostArch amd64

$llvmBin = 'C:\Program Files\LLVM\bin'
if (Test-Path (Join-Path $llvmBin 'libclang.dll')) {
    $env:LIBCLANG_PATH = $llvmBin
    $env:PATH = "$llvmBin;$env:PATH"
} else {
    Write-Warning "LLVMのlibclangが$llvmBinにありません。mozangleのbindgenが失敗する場合はLLVMをインストールしてください。"
}

if ($env:INCLUDE) {
    $parts = $env:INCLUDE -split ';' | ForEach-Object { $_.Trim() } | Where-Object { $_ -and (Test-Path $_) }
    if ($parts.Count -gt 0) {
        $env:BINDGEN_EXTRA_CLANG_ARGS = ($parts | ForEach-Object { "-isystem `"$_`"" }) -join ' '
    }
}
