import {expect, test} from 'bun:test';
import {matchedLocalGroup} from './ui';

const records = () => ['before', 'term-squid', 'reference'].map(label => ({label,
  manifest: {runId:label, suite:'local', status:'passed', dataset:{fingerprint:'same-data'},datasetLabel:'synthetic',
    localHarness:{sha256:'same-harness'},fhirVersion:'4.0.1',duration:'30s',warmup:'5s',client:{cpu:'test'},tests:['TS-LK01'],vus:[1,10]},
  points:[1,10].map(vus => ({test:'TS-LK01',vus,throughput:100*vus,error_rate:0,checks_pass:100,checks_fail:0,iterations:100,
    duration:{p50:1,p95:2,p99:3,avg:1.2}})),
}));
test('local charts preserve actual throughput rather than inventing composite scores', () => {
  const group = matchedLocalGroup(records());
  expect(group.runs[1].points[1].throughput).toBe(1000);
  expect(group).not.toHaveProperty('score');
  expect(group.vus).toEqual([1,10]);
});
test('local charts refuse mixed datasets, incomplete matrices and failed points', () => {
  const changed = records(); changed[1].manifest.dataset.fingerprint = 'different';
  expect(() => matchedLocalGroup(changed)).toThrow('Mismatched');
  const missing = records(); missing[0].points.pop();
  expect(() => matchedLocalGroup(missing)).toThrow('Incomplete');
  const failed = records(); failed[2].points[0].checks_fail = 1;
  expect(() => matchedLocalGroup(failed)).toThrow('failed');
  expect(() => matchedLocalGroup(records().slice(1))).toThrow('three');
});
test('empty or duplicate local test identities cannot produce a success chart', () => {
  const empty = records(); empty[0].manifest.tests = [];
  expect(() => matchedLocalGroup(empty)).toThrow('matrix');
  const duplicate = records(); duplicate[0].points[1] = duplicate[0].points[0];
  expect(() => matchedLocalGroup(duplicate)).toThrow('matrix');
});
