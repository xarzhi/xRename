@echo off
rem 方便在 cmd / 资源管理器里直接跑：pack 0.2.0
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0pack.ps1" %*
