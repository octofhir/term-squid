import {describe, expect, test} from 'bun:test';
import {targetUrl, selectedTests, validMeasurement, sourceFingerprint} from './bench';
import {CASES, accepts, resources, SYSTEM, code, display, VERSION, CONCEPT_COUNT} from './local/cases';

const outcomeResponse = (outcome: string) => ({
  resourceType: 'Parameters', parameter: [{name: 'outcome', valueCode: outcome}],
});
const subsumptionOutcomes = ['subsumes', 'subsumed-by', 'equivalent', 'not-subsumed'];

function fixtureSubsumption(path: string) {
  const params = new URL(path, 'http://localhost').searchParams;
  expect(params.get('system')).toBe(SYSTEM);
  expect(params.get('version')).toBe(VERSION);
  const a = params.get('codeA')!;
  const b = params.get('codeB')!;
  const known = (c: string) => c === 'root' || /^C\d{5}$/.test(c) && Number(c.slice(1)) < CONCEPT_COUNT;
  expect(known(a)).toBe(true);
  expect(known(b)).toBe(true);
  return a === b ? 'equivalent' : a === 'root' ? 'subsumes' : b === 'root' ? 'subsumed-by' : 'not-subsumed';
}

describe('benchmark correctness gates', () => {
  test('a code existing in a CodeSystem is insufficient for ValueSet membership', () => {
    const definition = CASES.find((c) => c.id === 'TS-VC01')!;
    const yes = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: true}]};
    const no = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: false}]};
    expect(accepts(definition.probes[0], 200, yes)).toBe(true);
    expect(accepts(definition.probes[1], 200, yes)).toBe(false);
    expect(accepts(definition.probes[2], 200, yes)).toBe(false);
    expect(accepts(definition.probes[1], 200, no)).toBe(true);
    expect(accepts(definition.probes[0], 200, {...yes, resourceType: undefined})).toBe(false);
    expect(accepts(definition.probes[0], 500, yes)).toBe(false);
  });
  test('membership load keeps a stable mix and rejects incorrect positive and negative responses', () => {
    const definition = CASES.find((c) => c.id === 'TS-VC01')!;
    const counts = {included: 0, excluded: 0, outside: 0, missing: 0};
    const seen = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      const p = definition.sample(i);
      const c = new URL(p.request.path, 'http://localhost').searchParams.get('code')!;
      seen.add(c);
      const n = Number(c.slice(1));
      const category = n >= 10000 ? 'missing' : n >= 256 ? 'outside' : n === 42 ? 'excluded' : 'included';
      counts[category]++;
      for (const valid of [true, false]) {
        const body = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: valid}]};
        expect(accepts(p, 200, body)).toBe(valid === (category === 'included'));
      }
    }
    expect(counts).toEqual({included: 400, excluded: 200, outside: 200, missing: 200});
    expect(seen.size).toBeGreaterThan(500);
  });
  test('translation load checks changing positive and missing codes in both directions', () => {
    for (const id of ['TS-CM01', 'TS-CM02']) {
      const definition = CASES.find((c) => c.id === id)!;
      const missing = definition.sample(0);
      const positive = definition.sample(1);
      const yes = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: true}]};
      const no = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: false}]};
      expect(accepts(missing, 200, no)).toBe(true);
      expect(accepts(missing, 200, yes)).toBe(false);
      expect(accepts(positive, 200, no)).toBe(false);
      const samples = Array.from({length: 100}, (_,i) => definition.sample(i));
      expect(samples.filter((p) => p.request.path.includes('missing-')).length).toBe(20);
      expect(new Set(samples.map((p) => p.request.path)).size).toBeGreaterThan(80);
    }
  });
  test('subsumption preflight covers all four outcomes and rejects malformed results', () => {
    const definition = CASES.find((c) => c.id === 'TS-SS01')!;
    const seen = new Set<string>();
    for (const p of definition.probes) {
      const expected = fixtureSubsumption(p.request.path);
      seen.add(expected);
      for (const outcome of subsumptionOutcomes) {
        expect(accepts(p, 200, outcomeResponse(outcome))).toBe(outcome === expected);
      }
      for (const invalid of [
        null,
        {resourceType: 'OperationOutcome', issue: []},
        {resourceType: 'Parameters', parameter: []},
        {resourceType: 'Parameters', parameter: [{name: 'outcome', valueString: expected}]},
        {resourceType: 'Parameters', parameter: {name: 'outcome', valueCode: expected}},
        {resourceType: 'Parameters', parameter: [null]},
        {resourceType: 'Parameters', parameter: [
          {name: 'outcome', valueCode: expected}, {name: 'outcome', valueCode: expected},
        ]},
        {resourceType: 'Parameters', parameter: [
          {name: 'outcome', valueCode: expected}, {name: 'outcome', valueCode: 'invalid'},
        ]},
      ]) expect(accepts(p, 200, invalid)).toBe(false);
      expect(accepts(p, 404, outcomeResponse(expected))).toBe(false);
      expect(accepts(p, 500, outcomeResponse(expected))).toBe(false);
    }
    expect(seen).toEqual(new Set(subsumptionOutcomes));
    // Preserve the original positive preflight rather than replacing it.
    const original = new URL(definition.probes[0].request.path, 'http://localhost').searchParams;
    expect(original.get('codeA')).toBe('root');
    expect(original.get('codeB')).toBe(code(7));
  });
  test('subsumption load has a stable mix with changing codes at every VU offset', () => {
    const definition = CASES.find((c) => c.id === 'TS-SS01')!;
    for (const start of [0, 137, 49 * 137, 4 * CONCEPT_COUNT - 4]) {
      const counts = Object.fromEntries(subsumptionOutcomes.map((outcome) => [outcome, 0]));
      const seen = new Set<string>();
      for (let i = start; i < start + 1000; i++) {
        const p = definition.sample(i);
        const expected = fixtureSubsumption(p.request.path);
        counts[expected]++;
        seen.add(p.request.path);
        for (const outcome of subsumptionOutcomes) {
          expect(accepts(p, 200, outcomeResponse(outcome))).toBe(outcome === expected);
        }
      }
      expect(counts).toEqual({'subsumes': 250, 'subsumed-by': 250, 'equivalent': 250, 'not-subsumed': 250});
      expect(seen.size).toBe(1000);
    }
    // Each outcome must use the full code pool, not just every fourth code.
    for (let offset = 0; offset < 4; offset++) {
      const codes = new Set(Array.from({length: CONCEPT_COUNT}, (_, n) => {
        const params = new URL(definition.sample(n * 4 + offset).request.path, 'http://localhost').searchParams;
        return params.get(offset === 0 ? 'codeB' : 'codeA');
      }));
      expect(codes.size).toBe(CONCEPT_COUNT);
    }
  });
  test('an expansion with duplicate or excluded codes cannot pass', () => {
    const p = CASES.find((c) => c.id === 'TS-EX01')!.probes[0];
    const contains = Array.from({length: 50}, (_, i) => ({system: SYSTEM, code: code(i < 42 ? i : i + 1)}));
    const body = {resourceType: 'ValueSet', expansion: {total: 255, offset: 0, contains}};
    expect(accepts(p, 200, body)).toBe(true);
    expect(accepts(p, 200, {...body, expansion: {...body.expansion, contains: Array(50).fill(contains[0])}})).toBe(false);
    contains[0] = {system: SYSTEM, code: code(42)};
    expect(accepts(p, 200, body)).toBe(false);
  });
  test('lookup cannot omit or substitute the terminology version', () => {
    const p = CASES.find((c) => c.id === 'TS-LK01')!.probes[0];
    const body = {resourceType: 'Parameters', parameter: [{name: 'display', valueString: display(7)}]};
    expect(accepts(p, 200, body)).toBe(false);
    body.parameter.push({name: 'version', valueString: VERSION});
    expect(accepts(p, 200, body)).toBe(true);
  });
  test('incomplete summaries and errors under load cannot count as valid speed results', () => {
    const result = {iterations: 100, checks_pass: 100, checks_fail: 0, error_rate: 0,
      throughput: 100, duration: {p50: 1, p95: 2, p99: 3, avg: 1.5}};
    expect(validMeasurement(result)).toBe(true);
    expect(validMeasurement({...result, checks_fail: 1})).toBe(false);
    expect(validMeasurement({...result, error_rate: 0.01})).toBe(false);
    expect(validMeasurement({...result, checks_fail: undefined})).toBe(false);
    expect(validMeasurement({...result, duration: {p50: 1, p95: 2}})).toBe(false);
    expect(validMeasurement({...result, iterations: 0})).toBe(false);
  });
  test('synthetic fixtures are deterministic and exercise a real exclusion', () => {
    expect(JSON.stringify(resources())).toBe(JSON.stringify(resources()));
    const vs: any = resources().find((r) => r.resourceType === 'ValueSet');
    expect(vs.compose.include[0].concept.some((c: any) => c.code === code(42))).toBe(true);
    expect(vs.compose.exclude[0].concept).toEqual([{code: code(42)}]);
  });
  test('harness provenance changes with source bytes and file boundaries', () => {
    const sources = [['cases.js', 'abc'], ['load.js', 'def']] as const;
    const hash = sourceFingerprint(sources);
    expect(hash).toMatch(/^[a-f0-9]{64}$/);
    expect(sourceFingerprint(sources)).toBe(hash);
    expect(sourceFingerprint([['cases.js', 'abC'], ['load.js', 'def']])).not.toBe(hash);
    expect(sourceFingerprint([['other.js', 'abc'], ['load.js', 'def']])).not.toBe(hash);
    expect(sourceFingerprint([['cases.js', 'ab'], ['load.js', 'cdef']])).not.toBe(hash);
  });
});
describe('benchmark target and scenario selection', () => {
  test('defaults can only apply load locally', () => {
    expect(targetUrl('http://127.0.0.1:18081/r4/')).toBe('http://127.0.0.1:18081/r4');
    expect(() => targetUrl('https://tx.fhir.org/r4')).toThrow('Local targets only');
    expect(targetUrl('https://my-terminology.example/r4', true)).toBe('https://my-terminology.example/r4');
    expect(() => targetUrl('http://user:password@localhost/r4')).toThrow('without credentials');
  });
  test('unknown tests cannot silently produce an empty successful run', () => {
    expect(selectedTests('TS-CS01,ts-cs01', ['TS-CS01'])).toEqual(['TS-CS01']);
    expect(() => selectedTests('typo', ['TS-CS01'])).toThrow('Unknown scenario');
  });
});
