@echo off
cd /d %~dp0
set PYTHONPATH=%~dp0src

REM DesktopBugCompanion.spec is the single definition of the build. The flags
REM used to be spelled out here and again in both GitHub workflows, with the
REM spec file sitting unused beside them, so four places could disagree about
REM what "the build" meant.

REM Same reason as run_dev.bat: a bare `python` is whichever environment the
REM shell was left in, which here would install the requirements into someone
REM else's venv and build from it.
set "PY=python"
if exist "%~dp0.venv\Scripts\python.exe" set "PY=%~dp0.venv\Scripts\python.exe"

"%PY%" -m pip install -r requirements.txt
if errorlevel 1 (
  echo Could not install requirements.
  pause
  exit /b 1
)

"%PY%" -m PyInstaller --noconfirm --clean DesktopBugCompanion.spec
if errorlevel 1 (
  echo Build failed.
  pause
  exit /b 1
)

echo.
echo Build complete.
echo Single-file executable: dist\DesktopBugCompanion.exe
echo Note: the app creates a presets folder next to the EXE when you save a preset.
pause
