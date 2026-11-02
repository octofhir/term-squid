#!/usr/bin/env python3
"""Record cumulative CPU time, RSS (KiB) and ps CPU percentage around a local command."""
import argparse
import json
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--pid', type=int, required=True)
parser.add_argument('--output', required=True)
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
if args.pid <= 0 or not args.command:
    parser.error('a positive PID and a command are required')
command = args.command[1:] if args.command[0] == '--' else args.command
child = subprocess.Popen(command)
rows = []
while True:
    result = subprocess.run(['ps', '-p', str(args.pid), '-o', 'time=,rss=,%cpu='], capture_output=True, text=True, check=False)
    rows.append({'at': time.time(), 'cpu_time_rss_kib_cpu_percent': result.stdout.strip()})
    if child.poll() is not None:
        break
    time.sleep(1)
with open(args.output, 'x') as file:
    json.dump(rows, file, indent=2)
raise SystemExit(child.returncode)
