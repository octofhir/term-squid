import {describe, expect, test} from 'bun:test';
import {targetUrl, selectedTests, validMeasurement} from './bench';
import {CASES, accepts, resources, SYSTEM, code, display, VERSION} from './local/cases';

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
