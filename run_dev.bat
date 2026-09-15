@echo off
cd /d %~dp0
set PYTHONPATH=%~dp0src
python -m desktop_bug.config_ui
pause
