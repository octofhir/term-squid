#!/usr/bin/env python3
"""Pinned official Java terminology tests, with an explicit preloaded-content profile."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
from urllib.parse import urlparse
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / 'benchmarks/ecosystem/lock.json'
WORK = ROOT / '.benchmarks/ecosystem'


def fingerprint(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def local_url(value):
    parsed = urlparse(value)
    if (parsed.scheme not in ('http', 'https') or parsed.hostname not in ('localhost', '127.0.0.1', '::1')
            or parsed.username or parsed.password or parsed.query or parsed.fragment):
        raise ValueError('Use a loopback HTTP(S) FHIR endpoint without credentials, query or fragment')
    return value.rstrip('/')


def artifact(spec, path, limit):
    if path.exists():
        if fingerprint(path) != spec['sha256']:
            raise ValueError(f'Cached artifact checksum mismatch: {path}')
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.download')
    try:
        with urlopen(spec['url'], timeout=120) as response, temporary.open('xb') as output:
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > limit:
                    raise ValueError('Pinned artifact exceeds download limit')
                output.write(chunk)
        if fingerprint(temporary) != spec['sha256']:
            raise ValueError(f'Download checksum mismatch: {spec["url"]}')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def preloaded_manifest(original, names):
    selected = json.loads(json.dumps(original))
    suites = {suite['name']: suite for suite in selected['suites']}
    if len(suites) != len(selected['suites']) or any(name not in suites for name in names):
        raise ValueError('Missing or duplicated selected suite')
    selected['suites'] = [suites[name] for name in names]
    setup = []
    for suite in selected['suites']:
        setup.extend(suite.get('setup', []))
        suite['setup'] = []
    return selected, sorted(set(setup))


def prepare():
    lock = json.loads(LOCK.read_text())
    WORK.mkdir(parents=True, exist_ok=True)
    jar = WORK / f'validator_cli-{lock["runner"]["version"]}.jar'
    package = WORK / f'ecosystem-{lock["suite"]["version"]}.tgz'
    artifact(lock['runner'], jar, 256 * 1024 * 1024)
    artifact(lock['suite'], package, 64 * 1024 * 1024)
    source = WORK / 'official'
    with tarfile.open(package) as archive:
        members = archive.getmembers()
        if len(members) > 20_000 or sum(m.size for m in members) > 256 * 1024 * 1024:
            raise ValueError('Official package exceeds extraction limits')
        # Always derive the managed source tree from the verified archive, including on cache hits.
        archive.extractall(source, filter='data')
    tests = source / 'package/tests'
    manifest, setup = preloaded_manifest(json.loads((tests / 'test-cases.json').read_text()), lock['profile']['suites'])
    # A separate generated tree preserves the downloaded package and every golden/request byte.
    destination = WORK / 'preloaded'
    shutil.copytree(tests, destination, dirs_exist_ok=True)
    (destination / 'test-cases.json').write_text(json.dumps(manifest, indent=2) + '\n')
    resources = WORK / 'setup/package'
    resources.mkdir(parents=True, exist_ok=True)
    plan = []
    hashes = {}
    for index, relative in enumerate(setup):
        file = (tests / relative).resolve()
        if not file.is_relative_to(tests.resolve()) or not file.is_file():
            raise ValueError('Invalid setup path in official manifest')
        target = resources / f'resource-{index:03}.json'
        plan.append(str(file) + '\t' + str(target))
        hashes[relative] = fingerprint(file)
    plan_file = WORK / 'convert.tsv'
    plan_file.write_text('\n'.join(plan) + '\n')
    home = WORK / 'java-home'
    home.mkdir(exist_ok=True)
    subprocess.run(['java', f'-Duser.home={home}', '-cp', str(jar),
                    str(ROOT / 'benchmarks/ecosystem/PrepareR4.java'), str(plan_file)], check=True)
    metadata = {'name': 'org.octofhir.ecosystem-setup', 'version': lock['suite']['version'],
                'fhirVersions': ['4.0.1'], 'type': 'fhir.ig', 'license': 'CC0-1.0'}
    (resources / 'package.json').write_text(json.dumps(metadata, indent=2) + '\n')
    upload = WORK / 'setup.tgz'
    with tarfile.open(upload, 'w:gz') as archive:
        for file in sorted(resources.glob('*.json')):
            content = file.read_bytes()
            info = tarfile.TarInfo('package/' + file.name)
            info.size, info.mtime, info.mode = len(content), 0, 0o644
            archive.addfile(info, io.BytesIO(content))
    source_hashes = {str(p.relative_to(tests)): fingerprint(p) for p in tests.rglob('*') if p.is_file()}
    for relative, expected in source_hashes.items():
        if relative != 'test-cases.json' and fingerprint(destination / relative) != expected:
            raise ValueError(f'Official request/golden content changed: {relative}')
    provenance = {'lock': lock, 'lockSha256': fingerprint(LOCK), 'setupSources': hashes,
                  'officialFiles': source_hashes, 'generatedManifestSha256': fingerprint(destination / 'test-cases.json'),
                  'convertedSetup': {p.name: fingerprint(p) for p in resources.glob('*.json')}}
    (WORK / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    print(f'Prepared {len(setup)} official setup resources; import {upload}', flush=True)
    return lock, jar, tests, destination


def results_summary(path):
    document = json.loads(path.read_text())
    tests = [test for suite in document['suites'] for test in suite.get('tests', [])]
    counts = {status: sum(test.get('status') == status for test in tests)
              for status in ('pass', 'fail', 'ignored')}
    counts['cases'] = [suite['name'] + '/' + test['name']
                       for suite in document['suites'] for test in suite.get('tests', [])]
    return counts


def expected_cases(original, names, modes, fhir_version):
    """Inventory the pinned runner's explicit suite/test mode and version selectors only."""
    result = []
    for suite in original['suites']:
        if suite['name'] not in names or suite.get('mode') not in (None, *modes):
            continue
        for test in suite['tests']:
            if test.get('mode') not in (None, *modes):
                continue
            selector = test.get('version')
            if selector:
                excluded = selector.startswith('!')
                version = selector.removeprefix('!')
                if version not in ('4.0', '5.0'):
                    raise ValueError('Unrecognized pinned corpus version selector')
                matches = fhir_version == version or fhir_version.startswith(version + '.')
                if matches == excluded:
                    continue
            result.append(suite['name'] + '/' + test['name'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'run'])
    parser.add_argument('--url', type=local_url)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--setup', choices=['preloaded', 'inline'], default='preloaded')
    parser.add_argument('--suite', help='One complete official suite, or all; nondefault suites require inline setup')
    args = parser.parse_args()
    if args.action == 'prepare':
        prepare()
        return
    if not args.url or not args.output:
        parser.error('run requires --url and --output')
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / '.benchmarks') or output.exists():
        parser.error('Use a new output directory under .benchmarks/')
    output.mkdir(parents=True)
    lock = json.loads(LOCK.read_text())
    jar = WORK / f'validator_cli-{lock["runner"]["version"]}.jar'
    # Run is offline after prepare; it neither downloads tests nor resolves a floating release.
    if not jar.is_file() or fingerprint(jar) != lock['runner']['sha256']:
        parser.error('Run prepare first; the pinned runner is missing or changed')
    original = json.loads((WORK / 'official/package/tests/test-cases.json').read_text())
    selected_suites = [args.suite] if args.suite and args.suite != 'all' else lock['profile']['suites']
    if args.suite == 'all':
        selected_suites = [suite['name'] for suite in original['suites']]
    if any(name not in {suite['name'] for suite in original['suites']} for name in selected_suites):
        parser.error('Unknown official suite')
    if args.setup == 'preloaded' and selected_suites != lock['profile']['suites']:
        parser.error('Nondefault suites require --setup inline; their setup must never be silently omitted')
    tests = WORK / ('preloaded' if args.setup == 'preloaded' else 'official/package/tests')
    provenance = json.loads((WORK / 'provenance.json').read_text())
    if provenance['lockSha256'] != fingerprint(LOCK):
        raise ValueError('Prepared profile does not match the current lock; run prepare')
    for relative, expected in provenance['officialFiles'].items():
        if relative == 'test-cases.json' and args.setup == 'preloaded':
            expected = provenance['generatedManifestSha256']
        if fingerprint(tests / relative) != expected:
            raise ValueError(f'Test source checksum mismatch: {relative}')
    with urlopen(args.url + '/metadata?_format=json', timeout=10) as response:
        metadata = json.load(response)
    if metadata.get('resourceType') != 'CapabilityStatement' or metadata.get('fhirVersion') != '4.0.1':
        raise ValueError('This profile requires a real R4 4.0.1 CapabilityStatement')
    settings = output / 'fhir-settings.json'
    settings.write_text(json.dumps({'servers': [{'url': args.url, 'allowHttp': True,
                                               'allowPrivateNetwork': True}]}, indent=2) + '\n')
    command = ['java', f'-Duser.home={WORK / "java-home"}', f'-Dfhir.settings.path={settings}', '-jar', str(jar), 'txTests',
               '-test-version', str(tests), '-tx', args.url, '-output', str(output),
               '-mode', ','.join(lock['profile']['modes'])]
    # Always select complete suites. Requests, setup, goldens and feature decisions stay upstream-owned.
    if len(selected_suites) == 1:
        command += ['-suite', selected_suites[0]]
    elif args.suite != 'all':
        raise ValueError('Use a single suite or the complete official inventory')
    with (output / 'runner.log').open('w') as log:
        result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    summary = results_summary(output / 'test-results.json') if (output / 'test-results.json').exists() else None
    manifest = {'lock': lock, 'setupMode': args.setup, 'selectedSuites': selected_suites, 'url': args.url, 'serverSoftware': metadata.get('software'),
                'command': command, 'exitCode': result.returncode, 'summary': summary, 'provenance': provenance}
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'exitCode': result.returncode, 'summary': summary, 'output': str(output)}), flush=True)
    expected = expected_cases(original, selected_suites, lock['profile']['modes'], metadata['fhirVersion'])
    if (result.returncode or summary is None or summary['pass'] != len(expected) or summary['fail']
            or summary['ignored'] or sorted(summary['cases']) != sorted(expected)):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
