#!/usr/bin/env bun
import {createHash, randomUUID} from 'node:crypto';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {dirname, join} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {fetchUpstream} from './bench';
import upstream from './upstream.json';

export interface PublishedPoint {
  rps: number; p50: number; p95: number; p99: number; errorRate: number;
}
export interface PublishedServer {
  id: string; version?: string; score: number; rawScore: number;
  preflight: Record<string, string>;
  benchmark: Record<string, Record<string, PublishedPoint>>;
  rawRps: Record<string, number>; imputedRps: Record<string, number>;
}
export interface PublishedRun {
  run: string; date: string; host: unknown;
  config: {vus: number[]; tests: string[]; testDuration: string; imputePercentile: number; bias: Record<string, number>};
  servers: PublishedServer[];
}
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0;

export function referenceReadings(run: PublishedRun) {
  return run.servers.flatMap(server => Object.entries(server.benchmark).flatMap(([test, levels]) => {
    if (!run.config.tests.includes(test) || server.preflight[test] !== 'pass') throw new Error(`Invalid measured test ${server.id}/${test}`);
    return Object.entries(levels).map(([vus, point]) => {
      if (!run.config.vus.includes(Number(vus)) || !['rps', 'p50', 'p95', 'p99', 'errorRate'].every(k => finite(point[k as keyof PublishedPoint])) ||
          point.errorRate > 1 || point.p50 > point.p95 || point.p95 > point.p99) {
        throw new Error(`Invalid measurement ${server.id}/${test}/${vus}`);
      }
      return {server: server.id, test, vus: Number(vus), ...point, effRps: point.rps * (1 - point.errorRate)};
    });
  }));
}
export function verifiedReference(source: string, expectedSha = upstream.reference.sha256): PublishedRun {
  if (createHash('sha256').update(source).digest('hex') !== expectedSha) throw new Error('Round 0 archive checksum mismatch.');
  const run = JSON.parse(source) as PublishedRun;
  if (run.run !== upstream.reference.run || run.date !== upstream.reference.date || !run.host ||
      JSON.stringify(run.config?.tests) !== JSON.stringify(upstream.tests) ||
      JSON.stringify(run.config?.vus) !== JSON.stringify(upstream.protocol.vus) ||
      run.config.testDuration !== upstream.protocol.duration || run.config.imputePercentile !== 15 ||
      !Array.isArray(run.servers) || !run.servers.length) throw new Error('Unexpected Round 0 archive schema/protocol.');
  const ids = new Set<string>();
  for (const server of run.servers) {
    if (!server.id || ids.has(server.id) || !finite(server.score) || server.score > 100 || !finite(server.rawScore) ||
        !server.preflight || !server.benchmark || !server.rawRps || !server.imputedRps) throw new Error('Invalid published server.');
    ids.add(server.id);
  }
  referenceReadings(run);
  return run;
}

async function main() {
  if (process.argv[2] === 'score' && process.argv.length === 4) {
    await fetchUpstream();
    const {scoreCohort} = await import('./composite');
    console.log(JSON.stringify(await scoreCohort(join(dirname(fileURLToPath(import.meta.url)), '..'), process.argv[3]), null, 2));
    return;
  }
  if (process.argv[2] !== 'reference' || process.argv.length !== 3) throw new Error('Usage: bun benchmarks/healthsamurai.ts reference | score RUN_ID');
  await fetchUpstream();
  const root = join(dirname(fileURLToPath(import.meta.url)), '..');
  const sourceDir = join(root, '.benchmarks/upstream');
  const published = verifiedReference(await readFile(join(sourceDir, upstream.reference.file), 'utf8'));
  // Execute the pinned original scorer; do not copy its implementation or add our
  // missing clinical cases to its imputed historical leaderboard.
  const {buildRawRps, computeScores} = await import(pathToFileURL(join(sourceDir, 'scripts/lib/scoring.ts')).href);
  const readings = referenceReadings(published);
  const result = computeScores(published.servers.map(s => s.id), published.config.tests, buildRawRps(readings), published.config.imputePercentile);
  for (const server of published.servers) {
    const rawScore = result.rawScores.get(server.id), score = result.scores.get(server.id);
    if (!finite(rawScore) || !finite(score) || Math.abs(rawScore - server.rawScore) > 0.01 || Math.abs(score - server.score) > 0.001) {
      throw new Error(`Published score cannot be reproduced for ${server.id}`);
    }
  }
  const directory = join(root, '.benchmarks/references', `round-0-${randomUUID()}`);
  await mkdir(directory, {recursive: true});
  await writeFile(join(directory, 'reference.json'), JSON.stringify({reference: upstream.reference, repository: upstream.repository,
    commit: upstream.commit, host: published.host, protocol: published.config, scoreReproduced: true,
    warning: 'Historical different-host/different-data reference, not a term-squid speedup or production measurement.',
    servers: published.servers.map(s => ({id: s.id, version: s.version, normalizedScore: s.score, weightedRps: s.rawScore,
      measuredTests: Object.keys(s.rawRps).length, imputedTests: Object.keys(s.imputedRps).length})), readings}, null, 2) + '\n');
  console.log(`Round 0: ${published.date}; original scores reproduced; ${readings.length} real per-VU readings.`);
  for (const s of published.servers) console.log(`${s.id}: ${s.version}; weighted=${s.rawScore.toFixed(1)}, score=${s.score}, real=${Object.keys(s.rawRps).length}/${published.config.tests.length}, imputed=${Object.keys(s.imputedRps).length}`);
  const termbox = readings.filter(p => p.server === 'termbox' && p.vus === 50);
  for (const p of termbox) console.log(`termbox ${p.test}, ${p.vus} VU: ${p.rps.toFixed(1)} actual RPS; p99=${p.p99.toFixed(3)} ms`);
  console.log(`Artifact: ${directory}\nNo production requests were sent. Weighted totals are not per-operation RPS.`);
}
if (import.meta.main) main().catch(error => {console.error(String(error)); process.exitCode = 1;});
