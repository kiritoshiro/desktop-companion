@echo off
cd /d %~dp0
set PYTHONPATH=%~dp0src

REM DesktopBugCompanion.spec is the single definition of the build. The flags
REM used to be spelled out here and again in both GitHub workflows, with the
REM spec file sitting unused beside them, so four places could disagree about
REM what "the build" meant.

python -m pip install -r requirements.txt
if errorlevel 1 (
  echo Could not install requirements.
  pause
  exit /b 1
)

python -m PyInstaller --noconfirm --clean DesktopBugCompanion.spec
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
