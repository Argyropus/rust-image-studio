@echo off
rem Build and install RustImageStudio.exe (see build.ps1).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1"
