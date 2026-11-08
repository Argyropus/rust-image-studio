# Builds the app and installs it as RustImageStudio.exe next to this script.
# Usage (cmd or PowerShell, in this folder):  build        (or: powershell -ExecutionPolicy Bypass -File build.ps1)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

Write-Host '빌드 중… (cargo build --release)' -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host '빌드 실패: 위의 오류를 확인하세요. 실행 파일은 바꾸지 않았습니다.' -ForegroundColor Red
    exit 1
}

$built = Join-Path $PSScriptRoot 'target\release\rust-image-studio.exe'
$dest = Join-Path $PSScriptRoot 'RustImageStudio.exe'
$old = Join-Path $PSScriptRoot 'RustImageStudio.old.exe'

# A running app locks its exe: it can't be overwritten, but it can be renamed aside.
if (Test-Path -LiteralPath $old) {
    try { Remove-Item -LiteralPath $old -Force } catch { }
}
try {
    Copy-Item -LiteralPath $built -Destination $dest -Force
}
catch {
    Rename-Item -LiteralPath $dest -NewName 'RustImageStudio.old.exe'
    Copy-Item -LiteralPath $built -Destination $dest -Force
    Write-Host '앱이 실행 중이라 기존 파일은 RustImageStudio.old.exe로 옮겼습니다. 앱을 다시 켜면 새 버전이 실행됩니다.' -ForegroundColor Yellow
}

$t = (Get-Item -LiteralPath $dest).LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss')
Write-Host "설치 완료: RustImageStudio.exe ($t)" -ForegroundColor Green
