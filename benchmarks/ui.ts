#!/usr/bin/env bun
import {cp, mkdir, readFile, writeFile} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {fetchUpstream, validMeasurement} from './bench';
import {verifiedReference} from './healthsamurai';
import upstream from './upstream.json';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// A production build must not replace Vite/Astro caches of an active dev server.
const SITE = join(ROOT, process.argv[2] === 'build' ? '.benchmarks/ui-build' : '.benchmarks/ui-site');
const LABELS = ['term-squid (before)', 'term-squid', 'FHIRsmith (local)'];
export function matchedLocalGroup(records: Array<{manifest: any; points: any[]; label: string}>) {
  if (records.length !== 3) throw new Error('A local comparison needs all three recorded servers.');
  const context = (m: any) => JSON.stringify([m.dataset?.fingerprint, m.datasetLabel, m.localHarness?.sha256,
    m.fhirVersion, m.duration, m.warmup, m.client?.cpu, m.tests, m.vus]);
  const first = records[0].manifest;
  if (!Array.isArray(first.tests) || !first.tests.length || !Array.isArray(first.vus) || !first.vus.length ||
      new Set(first.tests).size !== first.tests.length || new Set(first.vus).size !== first.vus.length) throw new Error('Missing local test/VU matrix.');
  for (const {manifest, points} of records) {
    if (manifest.suite !== 'local' || manifest.status !== 'passed' || !manifest.dataset?.fingerprint ||
        !manifest.localHarness?.sha256 || context(manifest) !== context(first)) throw new Error('Mismatched local run context.');
    const identities = points.map(p => `${p.test}/${p.vus}`);
    if (new Set(identities).size !== identities.length || points.length !== first.tests.length * first.vus.length ||
        points.some(p => !validMeasurement(p) || p.checks_pass !== p.iterations || !first.tests.includes(p.test) || !first.vus.includes(p.vus))) {
      throw new Error('Incomplete or failed local measurement matrix.');
    }
  }
  return {datasetLabel: first.datasetLabel, fingerprint: first.dataset.fingerprint, harness: first.localHarness.sha256,
    cpu: first.client.cpu, fhirVersion: first.fhirVersion, duration: first.duration, warmup: first.warmup,
    vus: first.vus, startedAt: first.startedAt, runs: records.map(r => ({label: r.label, id: r.manifest.runId,
      version: r.manifest.serverSoftware?.version, points: r.points}))};
}
async function localData(published: ReturnType<typeof verifiedReference>) {
  const groups = [], missing = [];
  for (const spec of [
    {id: 'concept', title: 'CodeSystem lookup and validation', prefix: 'concept-main', report: 'CONCEPT_READS_REPORT.md'},
    {id: 'generation', title: 'Prepared ValueSet, hierarchy and translation', prefix: 'generation-main', report: 'GENERATION_BATCHING_REPORT.md'},
  ]) {
    const records = [];
    for (const [index, role] of ['baseline', 'candidate', 'reference'].entries()) {
      const folder = join(ROOT, '.benchmarks/results', `${spec.prefix}-${role}`, role === 'reference' ? 'fhirsmith' : 'term-squid');
      if (!existsSync(join(folder, 'manifest.json'))) { missing.push(`${spec.prefix}-${role}`); continue; }
      const manifest = JSON.parse(await readFile(join(folder, 'manifest.json'), 'utf8'));
      const points = [];
      for (const entry of manifest.measurements) points.push(JSON.parse(await readFile(join(folder, 'benchmark', `${entry.test}_vus${entry.vus}.json`), 'utf8')));
      records.push({manifest, points, label: LABELS[index]});
    }
    if (records.length === 3) groups.push({...spec, ...matchedLocalGroup(records)});
  }
  const scoreFile = join(ROOT,'.benchmarks/composites/latest.json');
  const localComposite = existsSync(scoreFile) ? JSON.parse(await readFile(scoreFile,'utf8')) : null;
  const auditFile = ['rf2-final-upstream', 'inline-ucum-upstream-final', 'ucum-upstream-audit', 'inline-upstream-audit', 'healthsamurai-full-audit']
    .map(run => join(ROOT, '.benchmarks/results', run, 'term-squid/preflight.json')).find(file => existsSync(file));
  const audit = auditFile ? JSON.parse(await readFile(auditFile, 'utf8')) : null;
  return {generatedAt: new Date().toISOString(), groups, missing, localComposite, audit,
    referenceDate: published.date, referenceHost: published.host,
    composite: published.servers.map(server => ({id:server.id, version:server.version, score:server.score,
      weightedRps:server.rawScore, measured:Object.keys(server.rawRps).length, imputed:Object.keys(server.imputedRps).length})),
    warning: 'Local synthetic measurements. Not the clinical Round 0 workload; no composite score or cross-host speedup.'};
}
async function prepare() {
  await fetchUpstream();
  const source = join(ROOT, '.benchmarks/upstream/site');
  const published = verifiedReference(await readFile(join(ROOT, '.benchmarks/upstream', upstream.reference.file), 'utf8'));
  await mkdir(SITE, {recursive: true});
  await cp(source, SITE, {recursive: true, force: true, filter: path => !path.split('/').some(p => ['node_modules', 'dist', '.astro'].includes(p))});
  await cp(join(ROOT, 'benchmarks/ui/local.astro'), join(SITE, 'src/pages/local.astro'));
  await cp(join(ROOT, 'benchmarks/ui/LocalChart.svelte'), join(SITE, 'src/components/LocalChart.svelte'));
  const data = await localData(published);
  await writeFile(join(SITE, 'src/data/term-squid-local.json'), JSON.stringify(data, null, 2) + '\n');
  const baseFile = join(SITE, 'src/layouts/Base.astro');
  let base = await readFile(baseFile, 'utf8');
  base = base.replace('<nav class="site-nav">', '<nav class="site-nav">\n        <a href={`${base}/local/`} class="nav-link">Local term-squid</a>');
  // The local report must not require third-party font/CDN requests.
  base = base.split('\n').filter(line => !line.includes('fonts.googleapis.com') && !line.includes('fonts.gstatic.com') && !line.includes('cdn.jsdelivr.net')).join('\n');
  base = base.replace('</style>', '@media(max-width:640px){.header-inner{height:auto;min-height:52px;padding:10px 16px;flex-wrap:wrap;gap:8px}.site-nav{flex-wrap:wrap;max-width:100%}}\n</style>');
  await writeFile(baseFile, base);
  await writeFile(join(SITE, '.term-squid-ui.json'), JSON.stringify({upstreamCommit: upstream.commit, generatedAt: data.generatedAt}) + '\n');
  console.log(`Prepared ${data.groups.length} matched local comparisons; ${data.missing.length} missing source runs.`);
}
async function command(args: string[]) {
  const child = Bun.spawn(args, {cwd: SITE, stdin: 'inherit', stdout: 'inherit', stderr: 'inherit'});
  process.once('SIGINT', () => child.kill('SIGINT'));
  process.once('SIGTERM', () => child.kill('SIGTERM'));
  const code = await child.exited;
  if (code !== 0) throw new Error(`${args.join(' ')} failed (${code}).`);
}
async function main() {
  const action = process.argv[2] || 'serve';
  if (!['prepare', 'serve', 'build'].includes(action) || process.argv.length > 3) throw new Error('Usage: bun benchmarks/ui.ts [prepare|serve|build]');
  await prepare();
  if (action === 'prepare') return;
  await command(['bun', 'install', '--frozen-lockfile', '--ignore-scripts', '--cache-dir', join(ROOT, '.benchmarks/bun-cache')]);
  if (action === 'build') await command(['bun', 'run', 'build']);
  else {
    console.log('Local charts: http://127.0.0.1:4100/tx-benchmark/local/');
    await command(['bun', 'run', 'dev', '--host', '127.0.0.1', '--port', '4100']);
  }
}
if (import.meta.main) main().catch(error => {console.error(String(error)); process.exitCode = 1;});
