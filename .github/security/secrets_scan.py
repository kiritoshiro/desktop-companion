#!/usr/bin/env python3
"""Scan fetched history without verification or exposing detected secret values."""
import json
import os
from pathlib import Path
import subprocess
import sys

command = [os.environ.get('TRUFFLEHOG_PATH', 'trufflehog'),
           'git', Path.cwd().as_uri(), '--no-verification', '--no-update',
           '--json', '--fail', '--fail-on-scan-errors']
base, head = os.environ.get('SCAN_BASE', ''), os.environ.get('SCAN_HEAD', '')
if base and head:
    for revision in (base, head):
        subprocess.run(['git', 'cat-file', '-e', revision + '^{commit}'], check=True)
    command += ['--since-commit', base, '--branch', head]
result = subprocess.run(command, capture_output=True, text=True)
count = 0
for line in result.stdout.splitlines():
    try:
        finding = json.loads(line)
    except ValueError:
        print('Invalid scanner output; scan incomplete.', file=sys.stderr)
        sys.exit(2)
    count += 1
    # Raw/RawV2/ExtraData and commit messages must never be printed.
    meta = finding.get('SourceMetadata', {}).get('Data', {}).get('Git', {})
    print(f"Potential secret: {finding.get('DetectorName', 'unknown')} in {meta.get('file', 'unknown')}:{meta.get('line', '?')}")
print(f'TruffleHog: {count} potential secrets; verification disabled')
if result.returncode not in (0, 183):
    print(f'Secret scanner execution failed ({result.returncode}); raw output withheld.', file=sys.stderr)
    sys.exit(2)
sys.exit(1 if count or result.returncode else 0)
