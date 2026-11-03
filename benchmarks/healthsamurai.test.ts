import {describe, expect, test} from 'bun:test';
import {createHash} from 'node:crypto';
import {admittedTests, runProtocol, upstreamPreflightTests} from './bench';
import {referenceReadings, verifiedReference, type PublishedRun} from './healthsamurai';
import upstream from './upstream.json';

const fixture = (): PublishedRun => ({
  run: upstream.reference.run, date: upstream.reference.date, host: {cpu: 'test fixture, not benchmark hardware'},
  config: {vus: [1, 10, 50], tests: upstream.tests, testDuration: '30s', imputePercentile: 15, bias: {}},
  servers: [{id: 'fixture', score: 100, rawScore: 123000, preflight: {LK01: 'pass', LK02: 'skip'},
    benchmark: {LK01: {'1': {rps: 10, p50: 1, p95: 2, p99: 3, errorRate: 0},
      '50': {rps: 20, p50: 2, p95: 3, p99: 4, errorRate: 0.25}}},
    rawRps: {LK01: 999999}, imputedRps: {LK02: 888888}}],
});
const checked = (value: PublishedRun) => {
  const text = JSON.stringify(value);
  return verifiedReference(text, createHash('sha256').update(text).digest('hex'));
};

describe('HealthSamurai timing and clinical admission', () => {
  test('synthetic defaults remain unchanged and cannot be labeled Round 0 timing', () => {
    expect(runProtocol('local')).toEqual({duration: '10s', warmup: '3s', vus: [1, 10, 50], warmupVus: 1,
      warmupScope: 'per-test', interTestPauseMs: 0, timingMatchesRound0: false});
  });
  test('upstream defaults use the original global warmup and between-test delay', () => {
    expect(runProtocol('upstream')).toEqual({duration: '30s', warmup: '5s', vus: [1, 10, 50], warmupVus: 10,
      warmupScope: 'global', interTestPauseMs: 5000, timingMatchesRound0: true});
    expect(runProtocol('upstream', {duration: '1s'}).timingMatchesRound0).toBe(false);
    expect(runProtocol('upstream', {vus: '50'}).timingMatchesRound0).toBe(false);
    expect(runProtocol('upstream', {warmup: '0s'}).timingMatchesRound0).toBe(false);
  });
  test('invalid timing does not reach a load command', () => {
    for (const options of [{duration: '0s'}, {warmup: '-1s'}, {vus: '0'}, {vus: 'NaN'}, {vus: '10001'}]) {
      expect(() => runProtocol('upstream', options)).toThrow();
    }
    expect(() => runProtocol('unknown')).toThrow();
  });
  test('negative lookup requires positive SNOMED context even when selected alone', () => {
    expect(upstreamPreflightTests(['LK05'])).toEqual(['LK05', 'LK01']);
    expect(upstreamPreflightTests(['LK01', 'LK05'])).toEqual(['LK01', 'LK05']);
    expect(upstreamPreflightTests(['FS01'])).toEqual(['FS01']);
    for (const status of ['skip', 'fail', 'missing']) {
      const admission = admittedTests(['LK05'], {LK05: {status: 'pass'}, LK01: {status}}, 'upstream');
      expect(admission.passing).toEqual([]);
      expect(admission.blocked[0].dependency).toBe('LK01');
    }
    expect(admittedTests(['LK05'], {LK05: {status: 'pass'}, LK01: {status: 'pass'}}, 'upstream').passing).toEqual(['LK05']);
  });
  test('partial coverage stays partial without changing original pass/fail records', () => {
    const results = {FS01: {status: 'pass'}, LK01: {status: 'skip'}, LK05: {status: 'pass'}, CM02: {status: 'fail'}};
    expect(admittedTests(['FS01', 'LK01', 'LK05', 'CM02'], results, 'upstream').passing).toEqual(['FS01']);
    expect(results.LK05.status).toBe('pass');
    expect(admittedTests(['TS-LK01'], {'TS-LK01': {status: 'pass'}}, 'local').passing).toEqual(['TS-LK01']);
  });
});

describe('published Round 0 provenance and reading units', () => {
  test('archive checksum is required before parsing and protocol is pinned', () => {
    expect(() => verifiedReference(JSON.stringify(fixture()))).toThrow('checksum');
    expect(checked(fixture()).run).toBe('round-0');
    const changed = fixture(); changed.config.testDuration = '1s';
    expect(() => checked(changed)).toThrow('schema/protocol');
    const different = fixture(); different.config.tests = ['LK01'];
    expect(() => checked(different)).toThrow('schema/protocol');
  });
  test('weighted totals and imputed values never become actual request readings', () => {
    const rows = referenceReadings(checked(fixture()));
    expect(rows).toHaveLength(2);
    expect(rows.map(p => p.rps)).toEqual([10, 20]);
    expect(rows.map(p => p.effRps)).toEqual([10, 15]);
    expect(rows.some(p => p.test === 'LK02')).toBe(false);
  });
  test('invalid latency/error values and failed measured tests are rejected', () => {
    for (const change of [(p: any) => p.rps = -1, (p: any) => p.errorRate = 2, (p: any) => p.p95 = 0,
      (p: any) => p.p99 = Number.NaN]) {
      const value = fixture(); change(value.servers[0].benchmark.LK01['1']);
      expect(() => checked(value)).toThrow('Invalid measurement');
    }
    const failed = fixture(); failed.servers[0].preflight.LK01 = 'fail';
    expect(() => checked(failed)).toThrow('Invalid measured test');
  });
  test('duplicate server identities do not reproduce a misleading leaderboard', () => {
    const value = fixture(); value.servers.push(value.servers[0]);
    expect(() => checked(value)).toThrow('Invalid published server');
  });
});
