import {mkdir, readdir, readFile, writeFile} from 'node:fs/promises';
import {join, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {admittedTests, datasetIdentity, runProtocol, validMeasurement} from './bench';
import upstream from './upstream.json';

export function compositeReadings(records: Array<{manifest: any; points: any[]}>) {
  const ids = records.map(r => r.manifest.server);
  if (!['term-squid','termbox','fhirsmith'].every(id => ids.includes(id)) || new Set(ids).size !== ids.length) {
    throw new Error('A comparable cohort requires distinct term-squid, termbox and fhirsmith participants.');
  }
  const identity = (m: any) => JSON.stringify([m.dataIdentity?.sha256, m.environmentLabel, m.client?.cpu,
    m.client?.platform, m.client?.arch, m.upstreamHarness?.sha256]);
  const first = records[0].manifest;
  const protocol = runProtocol('upstream');
  const readings: Array<{server:string;test:string;effRps:number}> = [];
  for (const {manifest:m, points} of records) {
    if (m.suite !== 'upstream' || m.status !== 'passed' || !m.protocol?.timingMatchesRound0 ||
        m.upstream?.commit !== upstream.commit || !/^[a-f0-9]{64}$/.test(m.upstreamHarness?.sha256) ||
        !/^[a-f0-9]{64}$/.test(m.dataIdentity?.sha256) || !m.serverRevision ||
        !m.client?.cpu || !m.client?.platform || !m.client?.arch ||
        m.dataIdentity?.verification !== 'operator-attested' || !m.environmentLabel || identity(m) !== identity(first) ||
        m.duration !== protocol.duration || m.warmup !== protocol.warmup ||
        Object.entries(protocol).some(([key,value]) => JSON.stringify(m.protocol[key]) !== JSON.stringify(value)) ||
        JSON.stringify(m.tests) !== JSON.stringify(upstream.tests) || JSON.stringify(m.vus) !== JSON.stringify(upstream.protocol.vus)) {
      throw new Error('Composite needs full matched upstream runs, attested clinical data and recorded common environment; synthetic/historical mixtures are rejected.');
    }
    datasetIdentity(JSON.stringify(m.dataIdentity.data));
    const url = new URL(m.baseUrl);
    if (!['localhost','127.0.0.1','[::1]'].includes(url.hostname)) throw new Error('Only local cohorts may be scored here.');
    if (points.length !== upstream.tests.length * upstream.protocol.vus.length ||
        new Set(points.map(p=>`${p.test}/${p.vus}`)).size !== points.length) throw new Error('Incomplete composite matrix.');
    for (const p of points) {
      if (!upstream.tests.includes(p.test) || !upstream.protocol.vus.includes(p.vus) || !validMeasurement(p) ||
          p.server !== m.server || p.run !== m.runId || p.duration.p50 > p.duration.p95 || p.duration.p95 > p.duration.p99) {
        throw new Error('Failed/unknown composite point.');
      }
      readings.push({server:m.server,test:p.test,effRps:p.throughput*(1-p.error_rate)});
    }
  }
  return readings;
}

export async function scoreCohort(root: string, runId: string) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,99}$/.test(runId)) throw new Error('Invalid run ID.');
  const folder = join(root,'.benchmarks/results',runId);
  const records = [];
  for (const item of await readdir(folder,{withFileTypes:true})) {
    if (!item.isDirectory()) continue;
    const path = join(folder,item.name);
    const manifest = JSON.parse(await readFile(join(path,'manifest.json'),'utf8'));
    const preflight = JSON.parse(await readFile(join(path,'preflight.json'),'utf8'));
    if (manifest.runId !== runId || preflight.run !== runId || preflight.server !== manifest.server ||
        preflight.base_url !== manifest.baseUrl) throw new Error('Preflight/run identity mismatch.');
    if (admittedTests(upstream.tests,preflight.tests,'upstream').passing.length !== upstream.tests.length) throw new Error('Preflight coverage is not complete.');
    const points = [];
    for (const test of upstream.tests) for (const vus of upstream.protocol.vus) points.push(JSON.parse(await readFile(join(path,'benchmark',`${test}_vus${vus}.json`),'utf8')));
    records.push({manifest,points});
  }
  const readings = compositeReadings(records);
  const {buildRawRps,computeScores} = await import(pathToFileURL(resolve(root,'.benchmarks/upstream/scripts/lib/scoring.ts')).href);
  const result = computeScores(records.map(r=>r.manifest.server),upstream.tests,buildRawRps(readings),15);
  if (result.imputedRps.size || records.some(r => !Number.isFinite(result.scores.get(r.manifest.server)) ||
      !Number.isFinite(result.rawScores.get(r.manifest.server)))) throw new Error('Invalid composite result.');
  const data = {runId,formulaCommit:upstream.commit,dataIdentity:records[0].manifest.dataIdentity,
    environmentLabel:records[0].manifest.environmentLabel, client:records[0].manifest.client,
    protocol:records[0].manifest.protocol, upstreamHarness:records[0].manifest.upstreamHarness,
    readings, servers:records.map(r=>({id:r.manifest.server,revision:r.manifest.serverRevision,
      version:r.manifest.serverSoftware?.version,score:result.scores.get(r.manifest.server),
      weightedRps:result.rawScores.get(r.manifest.server),measured:20,imputed:0}))};
  const out = join(root,'.benchmarks/composites');await mkdir(out,{recursive:true});
  await writeFile(join(out,`${runId}.json`),JSON.stringify(data,null,2)+'\n');
  await writeFile(join(out,'latest.json'),JSON.stringify(data,null,2)+'\n');
  return data;
}
