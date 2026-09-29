#!/usr/bin/env python3
"""Run Semgrep CE and report locations without printing source or credentials."""
import json
import subprocess
import sys
from pathlib import Path

policy = json.loads(Path('.github/security/profile.json').read_text())
command = ['semgrep', 'scan', '--oss-only', '--metrics=off', '--disable-version-check',
           '--strict', '--error', '--json', '--exclude', '**/vendor/**',
           '--exclude', '**/node_modules/**', '--exclude', '**/*.min.js',
           '--exclude', '.github/security/**', '--exclude', 'tests/**']
for pack in policy['semgrep_rules']:
    command += ['--config', pack]
command += policy['source_paths']
result = subprocess.run(command, capture_output=True, text=True)
try:
    report = json.loads(result.stdout)
except (ValueError, TypeError):
    print('Semgrep did not return valid JSON; scan incomplete.', file=sys.stderr)
    sys.exit(2)
for finding in report.get('results', []):
    print(f"{finding['path']}:{finding['start']['line']}: {finding['check_id']}")
print(f"Semgrep: {len(report.get('results', []))} findings; {len(report.get('errors', []))} scan errors")
if result.returncode not in (0, 1) or report.get('errors'):
    print('Scanner/configuration/parse failure; inspect locally with the same profile.', file=sys.stderr)
    sys.exit(2)
sys.exit(1 if report.get('results') else result.returncode)
