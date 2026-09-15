@echo off
cd /d %~dp0
set PYTHONPATH=%~dp0src

python -m pip install -r requirements.txt

python -m PyInstaller ^
  --noconfirm ^
  --clean ^
  --onefile ^
  --windowed ^
  --name DesktopBugCompanion ^
  --paths src ^
  --add-data "models;models" ^
  --add-data "personalities;personalities" ^
  --add-data "presets;presets" ^
  --hidden-import desktop_bug.engine ^
  launcher.py

if errorlevel 1 (
  echo Build failed.
  pause
  exit /b 1
)

echo.
echo Build complete.
echo Single-file executable: dist\DesktopBugCompanion.exe
echo Note: the app may create a presets folder next to the EXE when you save or launch custom presets.
pause
