#!/usr/bin/env python3
"""Run original inline and preloaded R4 profiles, plus unchanged local gates, in both cache modes."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import urlparse
from urllib.request import urlopen
sys.dont_write_bytecode = True
from ecosystem import ROOT, WORK, prepare


@contextmanager
def backend(args, output, retention, phase):
    env = {key: value for key, value in os.environ.items() if not key.startswith('ADMIN_')}
    env.update(DATABASE_URL=args.database_url, SERVER_HOST='127.0.0.1', SERVER_PORT=str(args.port),
               TERMINOLOGY_PREPARED_CACHE=retention, RUST_LOG='warn')
    base = f'http://127.0.0.1:{args.port}'
    with (output / f'backend-{phase}-cache-{retention}.log').open('w') as log:
        server = subprocess.Popen([str(args.backend)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(300):
                if server.poll() is not None:
                    raise RuntimeError('Backend startup failed; see retained log')
                try:
                    with urlopen(base + '/health', timeout=1) as response:
                        if response.status == 200:
                            break
                except OSError:
                    time.sleep(.1)
            else:
                raise RuntimeError('Backend readiness deadline exceeded')
            yield base
        finally:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()


def inventory(base):
    counts = {}
    for family in ('CodeSystem', 'ValueSet', 'ConceptMap'):
        with urlopen(f'{base}/r4/{family}?_count=1', timeout=10) as response:
            bundle = json.load(response)
        if bundle.get('resourceType') != 'Bundle' or not isinstance(bundle.get('total'), int):
            raise RuntimeError('Invalid public inventory response')
        counts[family] = bundle['total']
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database-url', required=True)
    parser.add_argument('--backend', type=Path, default=ROOT / 'target/release/backend')
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release/term-squid-cli')
    parser.add_argument('--port', type=int, default=18084)
    parser.add_argument('--output', type=Path, default=ROOT / '.benchmarks/ecosystem-ci')
    args = parser.parse_args()
    if urlparse(args.database_url).hostname not in ('localhost', '127.0.0.1', '::1'):
        parser.error('Use a disposable loopback PostgreSQL database')
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / '.benchmarks') or output.exists():
        parser.error('Use a fresh output directory under .benchmarks/')
    output.mkdir(parents=True)
    prepare()
    subprocess.run([str(args.cli), '--database-url', args.database_url, 'migrate'], cwd=ROOT, check=True)
    outcomes = []
    # Original setup is sent as tx-resource by the unmodified official runner.
    # Verify the database is empty before and after, before importing any setup or fixtures.
    for retention in ['true', 'false']:
        with backend(args, output, retention, 'inline') as base:
            before = inventory(base)
            if any(before.values()):
                raise RuntimeError('Original inline proof requires a fresh empty terminology database')
            destination = output / f'inline-cache-{retention}'
            subprocess.run(['python3', 'benchmarks/ecosystem.py', 'run', '--setup', 'inline',
                            '--url', base + '/r4', '--output', str(destination)], cwd=ROOT, check=True)
            after = inventory(base)
            if after != before:
                raise RuntimeError('Temporary resources changed the public persisted inventory')
            official = json.loads((destination / 'manifest.json').read_text())
            outcomes.append({'setup': 'inline', 'retention': retention,
                             'officialCases': official['summary']['pass'], 'before': before, 'after': after})
    subprocess.run([str(args.cli), '--database-url', args.database_url, 'import', str(WORK / 'setup.tgz'), '-y'], cwd=ROOT, check=True)
    subprocess.run(['bun', 'benchmarks/bench.ts', 'fixtures'], cwd=ROOT, check=True)
    subprocess.run([str(args.cli), '--database-url', args.database_url, 'import', str(ROOT / '.benchmarks/fixtures/term-squid-benchmark.tgz'), '-y'], cwd=ROOT, check=True)
    for retention in ['true', 'false']:
        with backend(args, output, retention, 'preloaded') as base:
            subprocess.run(['python3', 'benchmarks/ecosystem.py', 'run', '--url', base + '/r4',
                            '--output', str(output / f'cache-{retention}')], cwd=ROOT, check=True)
            run_id = f'ecosystem-cache-{retention}-{time.time_ns()}'
            subprocess.run(['bun', 'benchmarks/bench.ts', 'run', '--url', base + '/r4', '--strict',
                            '--preflight-only', '--run-id', run_id], cwd=ROOT, check=True)
            official = json.loads((output / f'cache-{retention}/manifest.json').read_text())
            local = json.loads((ROOT / f'.benchmarks/results/{run_id}/term-squid/manifest.json').read_text())
            outcomes.append({'setup': 'preloaded', 'retention': retention, 'officialCases': official['summary']['pass'],
                             'localGates': local['coverage']['passed'], 'localRun': run_id})
    (output / 'summary.json').write_text(json.dumps(outcomes, indent=2) + '\n')


if __name__ == '__main__':
    main()
