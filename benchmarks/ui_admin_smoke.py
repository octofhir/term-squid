#!/usr/bin/env python3
"""Run the embedded production UI checks against a disposable local database.

Requires an already-built backend/CLI, generated local fixtures and Playwright Chromium.
All credentials, staging, screenshots and logs remain below .benchmarks/.
"""
import argparse
import os
from pathlib import Path
import secrets
import subprocess
import time
from urllib.parse import urlparse, urlencode
from urllib.request import urlopen, Request, build_opener, HTTPCookieProcessor
from http.cookiejar import CookieJar
import json
import io
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def recovery_check(server, backend, env, log, database_url, base, password, package=False):
    """Kill a real importer before commit and verify persistent restart recovery."""
    opener = build_opener(HTTPCookieProcessor(CookieJar()))
    def api(path, body=None, csrf=None):
        headers = {'Content-Type': 'application/gzip' if isinstance(body, bytes) else 'application/fhir+json', 'Origin': base}
        if csrf:
            headers['X-CSRF-Token'] = csrf
        request = Request(base + '/api/admin' + path,
                          data=None if body is None else body if isinstance(body, bytes) else json.dumps(body).encode(), headers=headers)
        with opener.open(request, timeout=10) as response:
            return json.load(response)
    def sql(command):
        subprocess.run(['psql', database_url, '-v', 'ON_ERROR_STOP=1', '-c', command], check=True, stdout=subprocess.DEVNULL)
    api('/login', {'username': 'admin', 'password': password})
    csrf = api('/session')['csrfToken']
    canonical = 'urn:term-squid:restart:' + secrets.token_hex(8)
    content = {'resourceType': 'CodeSystem', 'url': canonical, 'version': '1', 'status': 'active',
               'content': 'complete', 'concept': [{'code': 'recovered', 'display': 'Recovery fixture'}]}
    if package:
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode='w:gz', format=tarfile.USTAR_FORMAT) as archive:
            values = [('package/package.json', {'name': 'org.example.recovery', 'version': '1', 'fhirVersions': ['4.0.1']}),
                      ('package/CodeSystem.json', content),
                      ('package/ValueSet.json', {'resourceType': 'ValueSet', 'url': canonical + ':set', 'version': '1', 'status': 'active',
                                                'compose': {'include': [{'system': canonical}]}})]
            for path, value in values:
                raw = json.dumps(value).encode()
                member = tarfile.TarInfo(path)
                member.size = len(raw)
                archive.addfile(member, io.BytesIO(raw))
        upload = output.getvalue()
        (ROOT / '.benchmarks/ui-admin/smoke/recovery-package.tgz').write_bytes(upload)
    else:
        upload = content
    job = api('/jobs', upload, csrf)
    table = "value_sets" if package else "code_systems"
    sql(f"CREATE OR REPLACE FUNCTION ui_test_pause_import() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(10); RETURN NEW; END $$; CREATE TRIGGER ui_test_pause_import BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION ui_test_pause_import();")
    try:
        api('/jobs/' + job['id'] + '/start', {}, csrf)
        for _ in range(100):
            if api('/jobs/' + job['id'])['state'] == 'publishing':
                break
            time.sleep(0.1)
        else:
            raise RuntimeError('Importer did not enter publication')
        server.kill()
        server.wait(timeout=5)
    finally:
        sql(f'DROP TRIGGER ui_test_pause_import ON {table}; DROP FUNCTION ui_test_pause_import();')
    server = subprocess.Popen([str(backend)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(100):
            try:
                recovered = api('/jobs/' + job['id'])
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise RuntimeError('Restarted backend unavailable')
        if recovered['state'] != 'interrupted':
            raise RuntimeError('Unfinished import was not marked interrupted')
        with urlopen(base + '/api/catalog/CodeSystem?' + urlencode({'url': canonical}), timeout=10) as response:
            if json.load(response)['total'] != 0:
                raise RuntimeError('Interrupted import published content')
        if package:
            with urlopen(base + '/api/catalog/ValueSet?' + urlencode({'url': canonical + ':set'}), timeout=10) as response:
                if json.load(response)['total'] != 0:
                    raise RuntimeError('Interrupted package published a ValueSet')
        api('/logout', {}, csrf)
        print(f"PASS: killed {'package' if package else 'JSON'} importer recovered as interrupted; no content published", flush=True)
    except BaseException:
        server.terminate()
        server.wait(timeout=5)
        raise

    return server

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database-url', required=True)
    parser.add_argument('--backend', type=Path, default=ROOT / 'target/release/backend')
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release/term-squid-cli')
    parser.add_argument('--port', type=int, default=18083)
    args = parser.parse_args()
    if urlparse(args.database_url).hostname not in ('localhost', '127.0.0.1', '::1'):
        parser.error('Use a disposable loopback PostgreSQL database')
    folder = ROOT / '.benchmarks/ui-admin/smoke'
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    password = folder / 'password'
    password.write_text(secrets.token_urlsafe(32))
    password.chmod(0o600)
    phc = folder / 'admin.phc'
    result = subprocess.run([str(args.cli), 'hash-admin-password'], input=password.read_text(), text=True, capture_output=True, check=True)
    phc.write_text(result.stdout.strip())
    phc.chmod(0o600)
    subprocess.run([str(args.cli), '--database-url', args.database_url, 'migrate'], cwd=ROOT, check=True)
    subprocess.run([str(args.cli), '--database-url', args.database_url, 'import', str(ROOT / '.benchmarks/fixtures/term-squid-benchmark.tgz'), '-y'], cwd=ROOT, check=True)
    base = f'http://127.0.0.1:{args.port}'
    env = {**os.environ, 'DATABASE_URL': args.database_url, 'SERVER_HOST': '127.0.0.1', 'SERVER_PORT': str(args.port),
           'ADMIN_ORIGIN': base, 'ADMIN_USERNAME': 'admin', 'ADMIN_PASSWORD_HASH_FILE': str(phc), 'ADMIN_STAGING_DIR': str(folder / 'staging'),
           'ADMIN_SESSION_SECONDS': '30', 'RUST_LOG': 'info'}
    with (folder / 'backend.log').open('w') as log:
        server = subprocess.Popen([str(args.backend)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(150):
                if server.poll() is not None:
                    raise RuntimeError('Backend exited; inspect .benchmarks/ui-admin/smoke/backend.log')
                try:
                    with urlopen(f'{base}/health', timeout=1) as response:
                        if response.status == 200:
                            break
                except OSError:
                    time.sleep(0.1)
            else:
                raise RuntimeError('Backend readiness deadline exceeded')
            server = recovery_check(server, args.backend, env, log, args.database_url, base, password.read_text())
            server = recovery_check(server, args.backend, env, log, args.database_url, base, password.read_text(), package=True)
            # Start browser scenarios with a fresh credential-attempt fixture. Rate-limit
            # behavior is asserted independently by the PostgreSQL administration tests.
            subprocess.run(['psql', args.database_url, '-v', 'ON_ERROR_STOP=1', '-c', 'UPDATE admin_login_limit SET attempts=0,window_start=NOW()'], check=True, stdout=subprocess.DEVNULL)
            test_env = {**os.environ, 'UI_BASE_URL': base, 'ADMIN_TEST_PASSWORD_FILE': str(password), 'TEST_SHORT_SESSION': '1'}
            subprocess.run(['pnpm', 'exec', 'playwright', 'test'], cwd=ROOT / 'frontend', env=test_env, check=True)
            subprocess.run(['bun', 'benchmarks/bench.ts', 'run', '--url', base + '/r4', '--preflight-only', '--strict', '--run-id', 'ui-admin-' + str(time.time_ns())], cwd=ROOT, check=True)
        finally:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()


if __name__ == '__main__':
    main()
