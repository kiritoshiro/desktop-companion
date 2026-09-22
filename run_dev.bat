@echo off
cd /d %~dp0
set PYTHONPATH=%~dp0src

REM Prefer the project's own virtual environment over whatever `python`
REM happens to resolve to. A bare `python` picks up any activated venv, so
REM this script failed with "No module named 'PyQt5'" while the .venv sitting
REM right beside it had PyQt5 installed all along -- the launcher should not
REM depend on which environment the shell was left in.
set "PY=python"
if exist "%~dp0.venv\Scripts\python.exe" set "PY=%~dp0.venv\Scripts\python.exe"

"%PY%" -m desktop_bug.app.config_ui
pause
