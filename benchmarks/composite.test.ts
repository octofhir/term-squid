import {describe, expect, test} from 'bun:test';
import {compositeReadings} from './composite';
import {datasetIdentity, runProtocol} from './bench';
import upstream from './upstream.json';
import template from './round0-dataset.template.json';

const corpus = () => ({profile: 'round-0', attestedLoaded: true,
  sources: upstream.datasetSources.map(source => ({...source, sha256: 'a'.repeat(64)}))});
const cohort = () => ['term-squid', 'termbox', 'fhirsmith'].map((server, i) => ({
  manifest: {server, runId: 'fixture', serverRevision: 'fixture-revision', suite: 'upstream', status: 'passed',
    baseUrl: `http://127.0.0.1:${18081 + i}/r4`, protocol: runProtocol('upstream'),
    duration: '30s', warmup: '5s', upstream: {commit: upstream.commit},
    upstreamHarness: {sha256: 'b'.repeat(64)}, dataIdentity: datasetIdentity(JSON.stringify(corpus())),
    environmentLabel: 'fixture environment, not a benchmark measurement',
    client: {cpu: 'fixture CPU', platform: 'fixture', arch: 'fixture'}, tests: [...upstream.tests], vus: [1,10,50]},
  points: upstream.tests.flatMap(test => [1,10,50].map(vus => ({server, run: 'fixture', test, vus,
    throughput: (i + 1) * vus, error_rate: 0, iterations: 100, checks_pass: 200, checks_fail: 0,
    duration: {p50: 1, p95: 2, p99: 3, avg: 1.5}}))),
}));

describe('clinical corpus identity', () => {
  test('an unfilled template is not proof of loaded data', () => {
    expect(() => datasetIdentity(JSON.stringify(template))).toThrow();
    const data = corpus();
    expect(datasetIdentity(JSON.stringify(data)).verification).toBe('operator-attested');
    for (const change of [(d: any) => d.attestedLoaded = false, (d: any) => d.sources.pop(),
      (d: any) => d.sources[0].version = 'other', (d: any) => d.sources[0].sha256 = '',
      (d: any) => d.sources.push(d.sources[0]), (d: any) => d.sources[1] = d.sources[0]]) {
      const invalid = corpus(); change(invalid);
      expect(() => datasetIdentity(JSON.stringify(invalid))).toThrow();
    }
  });
});

describe('matched clinical composite admission', () => {
  test('only real readings reach the original scorer; checks may exceed iterations', () => {
    const rows = compositeReadings(cohort());
    expect(rows).toHaveLength(180);
    expect(rows[0]).toEqual({server: 'term-squid', test: 'FS01', effRps: 1});
    expect(rows[2].effRps).toBe(50);
    expect(rows.at(-1)?.effRps).toBe(150);
  });
  test('synthetic, partial, unrecorded and different-environment runs cannot claim a comparable score', () => {
    const changes = [(m: any) => m.suite = 'local', (m: any) => m.status = 'partial',
      (m: any) => m.protocol.duration = '1s', (m: any) => m.duration = '1s',
      (m: any) => m.protocol.warmupVus = 1, (m: any) => m.tests.pop(),
      (m: any) => m.upstream.commit = 'other', (m: any) => m.upstreamHarness.sha256 = 'c'.repeat(64),
      (m: any) => m.dataIdentity.sha256 = 'd'.repeat(64), (m: any) => m.dataIdentity.data.sources.pop(),
      (m: any) => m.dataIdentity.verification = 'unknown', (m: any) => m.environmentLabel = 'other host',
      (m: any) => m.client.cpu = 'other CPU', (m: any) => m.client.platform = '',
      (m: any) => m.serverRevision = '', (m: any) => m.baseUrl = 'https://production.example/fhir'];
    for (const change of changes) {
      const records = cohort(); change(records[1].manifest);
      expect(() => compositeReadings(records)).toThrow();
    }
  });
  test('both named competitors and distinct identities are required', () => {
    expect(() => compositeReadings(cohort().slice(0,2))).toThrow();
    const duplicate = cohort(); duplicate[2].manifest.server = 'termbox';
    expect(() => compositeReadings(duplicate)).toThrow();
    expect(() => compositeReadings([])).toThrow();
  });
  test('missing, duplicate, wrong-owner and failed measurements are rejected', () => {
    for (const change of [(r: any) => r.points.pop(), (r: any) => r.points[1] = r.points[0],
      (r: any) => r.points[0].checks_fail = 1, (r: any) => r.points[0].error_rate = 0.01,
      (r: any) => r.points[0].throughput = Infinity, (r: any) => r.points[0].test = 'TS-LK01',
      (r: any) => r.points[0].server = 'other', (r: any) => r.points[0].run = 'other',
      (r: any) => r.points[0].duration.p99 = 0]) {
      const records = cohort(); change(records[0]);
      expect(() => compositeReadings(records)).toThrow();
    }
  });
});
