#!/usr/bin/env bun
import {createHash, randomUUID} from 'node:crypto';
import {existsSync} from 'node:fs';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {cpus, platform, arch} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {parseArgs} from 'node:util';
import {fileURLToPath} from 'node:url';
import {CASES, accepts, resources, CONCEPT_COUNT, VERSION} from './local/cases.js';
import upstream from './upstream.json';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WORK = join(ROOT, '.benchmarks');
const SOURCE = join(WORK, 'upstream');
const ALL_UPSTREAM = upstream.tests;

async function command(args: string[], cwd = ROOT, env = process.env) {
  const proc = Bun.spawn(args, {cwd, env, stdin: 'ignore', stdout: 'pipe', stderr: 'pipe'});
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
  ]);
  return {stdout, stderr, exitCode};
}
async function required(args: string[], cwd = ROOT, env = process.env) {
  const result = await command(args, cwd, env);
  if (result.exitCode !== 0) throw new Error(`${args[0]} failed:\n${result.stderr || result.stdout}`);
  return result.stdout.trim();
}
const json = async (path: string) => JSON.parse(await readFile(path, 'utf8'));
const save = async (path: string, value: unknown) => writeFile(path, JSON.stringify(value, null, 2) + '\n');

export function targetUrl(input: string, allowRemote = false) {
  const url = new URL(input);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
    throw new Error('Use an HTTP(S) FHIR base URL without credentials, query or fragment.');
  }
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (!local && !allowRemote) throw new Error('Local targets only by default. Use --allow-remote for your own remote server.');
  return url.toString().replace(/\/$/, '');
}
export function selectedTests(input: string | undefined, available: string[]) {
  const selected = input ? [...new Set(input.split(',').map((s) => s.trim().toUpperCase()))] : available;
  if (!selected.length || selected.some((id) => !available.includes(id))) {
    throw new Error(`Unknown scenario. Available: ${available.join(', ')}`);
  }
  return selected;
}
export function validMeasurement(result: any) {
  return result?.iterations > 0 && result.checks_pass > 0 && result.checks_fail === 0 &&
    result.error_rate === 0 && Number.isFinite(result.throughput) && result.throughput > 0 &&
    ['p50', 'p95', 'p99', 'avg'].every((key) => Number.isFinite(result.duration?.[key]) && result.duration[key] >= 0);
}
function identifier(value: string, label: string) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,99}$/.test(value)) throw new Error(`Invalid ${label}. Use letters, numbers, '.', '_' and '-'.`);
  return value;
}

async function fetchUpstream() {
  await mkdir(WORK, {recursive: true});
  if (!existsSync(SOURCE)) await required(['git', 'clone', '--no-checkout', '--filter=blob:none', upstream.repository, SOURCE]);
  const dirty = await required(['git', 'status', '--porcelain', '--untracked-files=no'], SOURCE);
  // A newly cloned, not-yet-checked-out tree appears deleted in git status.
  const head = await command(['git', 'rev-parse', 'HEAD'], SOURCE);
  const checkedOut = existsSync(join(SOURCE, 'package.json'));
  if (checkedOut && dirty) throw new Error(`Tracked files changed in ${SOURCE}; preserve those changes before updating the benchmark.`);
  const known = await command(['git', 'cat-file', '-e', `${upstream.commit}^{commit}`], SOURCE);
  if (known.exitCode !== 0) await required(['git', 'fetch', '--depth', '1', 'origin', upstream.commit], SOURCE);
  if (!checkedOut || head.stdout.trim() !== upstream.commit) await required(['git', 'checkout', '--detach', upstream.commit], SOURCE);
  const actual = await required(['git', 'rev-parse', 'HEAD'], SOURCE);
  if (actual !== upstream.commit) throw new Error(`Unexpected upstream revision: ${actual}`);
  console.log(`TX Benchmark: ${actual} (${SOURCE})`);
}

async function fixtures() {
  const directory = join(WORK, 'fixtures');
  const packageDir = join(directory, 'package');
  await mkdir(packageDir, {recursive: true});
  const data = resources();
  const fingerprint = createHash('sha256').update(JSON.stringify(data)).digest('hex');
  for (const resource of data) await save(join(packageDir, `${resource.resourceType}-${resource.id}.json`), resource);
  await save(join(packageDir, 'package.json'), {
    name: 'org.octofhir.term-squid.benchmark', version: VERSION, type: 'fhir.ig',
    canonical: 'http://example.org/term-squid-benchmark', fhirVersions: ['4.0.1'],
    description: 'Synthetic local benchmark fixtures; no clinical terminology content.',
  });
  const dataset = {id: 'term-squid-local-v1', version: VERSION, conceptCount: CONCEPT_COUNT,
    fingerprint, resources: data.map((r) => ({resourceType: r.resourceType, url: r.url, version: r.version}))};
  await save(join(directory, 'dataset.json'), dataset);
  const archive = join(directory, 'term-squid-benchmark.tgz');
  // macOS AppleDouble metadata ends in .json too; keep it out of FHIR packages.
  await required(['tar', '-czf', archive, '-C', directory, 'package'], ROOT, {...process.env, COPYFILE_DISABLE: '1'});
  console.log(`Fixtures: ${archive}\nDataset SHA-256: ${fingerprint}`);
  return {archive, dataset};
}

async function response(base: string, request: {path: string; method?: string; body?: string}) {
  const res = await fetch(base + request.path, {
    method: request.method || 'GET', body: request.body,
    headers: {Accept: 'application/fhir+json', ...(request.body ? {'Content-Type': 'application/fhir+json'} : {})},
    signal: AbortSignal.timeout(10000),
  });
  const text = await res.text();
  let body: any;
  try { body = JSON.parse(text); } catch { body = text.slice(0, 1000); }
  return {status: res.status, body};
}

async function run(options: Record<string, any>) {
  const suite = options.suite || 'local';
  if (!['local', 'upstream'].includes(suite)) throw new Error('--suite must be local or upstream.');
  if (suite === 'upstream' && !options['preflight-only'] && !options['dataset-label']) {
    throw new Error('Upstream load runs require --dataset-label with the loaded terminology/package versions.');
  }
  const base = targetUrl(options.url || process.env.BENCH_URL || 'http://127.0.0.1:18081/r4', options['allow-remote']);
  const name = identifier(options.name || 'term-squid', 'server name');
  const runId = identifier(options['run-id'] || `${new Date().toISOString().replace(/[:.]/g, '-')}-${randomUUID().slice(0, 8)}`, 'run ID');
  const tests = selectedTests(options.tests, suite === 'local' ? CASES.map((c) => c.id) : ALL_UPSTREAM);
  const vus = [...new Set(String(options.vus || '1,10,50').split(',').map(Number))];
  if (vus.some((v) => !Number.isInteger(v) || v < 1 || v > 10000)) throw new Error('--vus must contain positive integers up to 10000.');
  const duration = options.duration || '10s';
  const warmup = options.warmup || '3s';
  if (!/^[1-9]\d*(ms|s|m)$/.test(duration) || !/^\d+(ms|s|m)$/.test(warmup)) throw new Error('Use durations such as 10s or 1m. Warmup may be 0s.');
  const k6Version = await required(['k6', 'version']);
  const metadata = await response(base, {path: '/metadata'});
  if (metadata.status !== 200 || metadata.body?.resourceType !== 'CapabilityStatement') {
    throw new Error(`No FHIR CapabilityStatement at ${base}/metadata (HTTP ${metadata.status}). Start the server and check the base path.`);
  }
  if (suite === 'local' && metadata.body.fhirVersion !== '4.0.1') throw new Error('The local fixture suite targets FHIR R4 (4.0.1).');
  if (suite === 'upstream') await fetchUpstream();
  const out = join(WORK, 'results', runId, name);
  if (existsSync(out)) throw new Error(`Results already exist: ${out}. Choose another --run-id.`);
  await mkdir(join(out, 'benchmark'), {recursive: true});
  await mkdir(join(out, 'logs'), {recursive: true});
  const fixtureData = suite === 'local' ? (await fixtures()).dataset : null;
  const manifest: any = {
    runId, suite, server: name, baseUrl: base, startedAt: new Date().toISOString(), status: 'running',
    client: {bun: Bun.version, k6: k6Version, platform: platform(), arch: arch(), cpu: cpus()[0]?.model},
    runnerCommit: await required(['git', 'rev-parse', 'HEAD']),
    runnerDirty: !!(await required(['git', 'status', '--porcelain'])),
    serverSoftware: metadata.body.software, fhirVersion: metadata.body.fhirVersion,
    serverRevision: options['server-revision'] || null,
    dataset: fixtureData || options['dataset-label'] || 'External dataset: record exact terminology and package versions before comparing runs.',
    datasetLabel: options['dataset-label'] || null,
    upstream: suite === 'upstream' ? upstream : null, tests, vus, duration, warmup, measurements: [],
  };
  await save(join(out, 'manifest.json'), manifest);
  const env = {...process.env, BASE_URL: base, SERVER_NAME: name, RUN_ID: runId};
  async function k6(args: string[], logName: string, extraEnv = {}) {
    const result = await command(['k6', 'run', '--quiet', '--no-usage-report', ...args], WORK, {...env, ...extraEnv});
    await writeFile(join(out, 'logs', logName), result.stdout + result.stderr);
    if (result.exitCode !== 0) throw new Error(`k6 failed (${result.exitCode}); see ${join(out, 'logs', logName)}`);
  }
  try {
    let preflight: any;
    if (suite === 'local') {
      const results: Record<string, any> = {};
      for (const id of tests) {
        const definition = CASES.find((c) => c.id === id)!;
        const failures = [];
        for (const probe of definition.probes) {
          try {
            const actual = await response(base, probe.request);
            if (!accepts(probe, actual.status, actual.body)) failures.push({expectation: probe.expectation, request: probe.request,
              status: actual.status, response: JSON.stringify(actual.body).slice(0, 2000)});
          } catch (error) { failures.push({expectation: probe.expectation, error: String(error)}); }
        }
        results[id] = failures.length ? {status: 'fail', failed_checks: failures} : {status: 'pass'};
      }
      preflight = {run: runId, server: name, base_url: base, timestamp: new Date().toISOString(), tests: results};
      await save(join(out, 'preflight.json'), preflight);
    } else {
      await k6([join(SOURCE, 'preflight/run.js')], 'preflight.log', {TESTS_FILTER: tests.join(',')});
      preflight = await json(join(out, 'preflight.json'));
    }
    const passing = tests.filter((id) => preflight.tests[id]?.status === 'pass');
    for (const id of tests) console.log(`${id}: ${preflight.tests[id]?.status || 'missing'}`);
    manifest.passingTests = passing;
    manifest.coverage = {passed: passing.length, selected: tests.length};
    if (!passing.length || (options.strict && passing.length !== tests.length)) {
      manifest.status = 'preflight-failed';
      throw new Error(`Preflight: ${passing.length}/${tests.length} passed. Failing scenarios were not benchmarked. See ${out}`);
    }
    if (!options['preflight-only']) {
      for (const id of passing) {
        const script = suite === 'local' ? join(ROOT, 'benchmarks/local/load.js') :
          join(SOURCE, 'k6', id.slice(0, 2), `${id}.js`);
        if (!/^0(ms|s|m)$/.test(warmup)) {
          await k6(['--vus', '1', '--duration', warmup, script], `${id}-warmup.log`, {
            TEST_ID: id, VUS: '1', RESULT_FILE: join(out, 'warmup.json'),
          });
        }
        for (const vu of vus) {
          const file = join(out, 'benchmark', `${id}_vus${vu}.json`);
          await k6(['--vus', String(vu), '--duration', duration, script], `${id}-vus${vu}.log`, {
            TEST_ID: id, VUS: String(vu), RESULT_FILE: file,
          });
          const result = await json(file);
          // Upstream k6 checks do not themselves cause a non-zero exit code.
          if (!validMeasurement(result)) {
            throw new Error(`${id} failed under load; invalid speed result is retained for diagnostics in ${file}`);
          }
          manifest.measurements.push({test: id, vus: vu, file});
          console.log(`${id}, ${vu} VU: ${result.throughput.toFixed(1)} req/s, p95 ${result.duration.p95.toFixed(2)} ms`);
        }
      }
    }
    manifest.status = passing.length === tests.length ? 'passed' : 'partial';
    console.log(`Results: ${out}`);
  } catch (error) {
    if (manifest.status === 'running') manifest.status = 'failed';
    manifest.error = String(error);
    throw error;
  } finally {
    manifest.finishedAt = new Date().toISOString();
    await save(join(out, 'manifest.json'), manifest);
  }
}

async function main() {
  const [action = 'help', ...args] = process.argv.slice(2);
  const {values} = parseArgs({args, options: {
    suite: {type: 'string'}, url: {type: 'string'}, name: {type: 'string'}, tests: {type: 'string'},
    vus: {type: 'string'}, duration: {type: 'string'}, warmup: {type: 'string'}, 'run-id': {type: 'string'},
    'dataset-label': {type: 'string'}, strict: {type: 'boolean'}, 'preflight-only': {type: 'boolean'},
    'server-revision': {type: 'string'},
    'allow-remote': {type: 'boolean'}, help: {type: 'boolean'},
  }});
  if (values.help || action === 'help') {
    console.log('Usage: bun benchmarks/bench.ts <fixtures|seed|fetch|run> [options]\n' +
      'run: --suite local|upstream --url URL --name NAME --tests ID,ID --vus 1,10,50\n' +
      '     --duration 10s --warmup 3s --preflight-only --strict --run-id ID --dataset-label TEXT\n' +
      'Targets default to local /r4. --allow-remote opts in to load on your own remote server.\n' +
      `Local scenarios: ${CASES.map((c) => c.id).join(', ')}`);
  } else if (action === 'fixtures') await fixtures();
  else if (action === 'fetch') await fetchUpstream();
  else if (action === 'seed') {
    const db = process.env.DATABASE_URL;
    if (!db) throw new Error('Set DATABASE_URL to the benchmark database before seeding.');
    const {archive} = await fixtures();
    const result = await command(['cargo', 'run', '--release', '--bin', 'term-squid-cli', '--',
      '--database-url', db, 'import', archive, '-y']);
    console.log(result.stdout);
    if (result.exitCode !== 0) throw new Error(result.stderr);
    console.log('Benchmark fixtures imported. Re-importing the same version is idempotent.');
  } else if (action === 'run') await run(values);
  else throw new Error(`Unknown action: ${action}`);
}
if (import.meta.main) main().catch((error) => {console.error(String(error)); process.exitCode = 1;});
