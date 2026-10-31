#!/usr/bin/env python3
"""Run Semgrep CE and report locations without printing source or credentials."""
import fnmatch
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
# Reviewed exceptions: a rule in the listed files only, each justified in
# docs/cybersecurity.md. The same rule anywhere else still fails the scan.
reviewed = policy.get('reviewed_findings', [])


def is_reviewed(finding):
    return any(finding['check_id'] == entry['rule']
               and any(fnmatch.fnmatch(finding['path'], pattern) for pattern in entry['paths'])
               for entry in reviewed)


results = [finding for finding in report.get('results', []) if not is_reviewed(finding)]
for finding in results:
    print(f"{finding['path']}:{finding['start']['line']}: {finding['check_id']}")
print(f"Semgrep: {len(results)} findings ({len(report.get('results', [])) - len(results)} reviewed); "
      f"{len(report.get('errors', []))} scan errors")
if result.returncode not in (0, 1) or report.get('errors'):
    print('Scanner/configuration/parse failure; inspect locally with the same profile.', file=sys.stderr)
    sys.exit(2)
sys.exit(1 if results else 0)
