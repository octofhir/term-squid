import { expect, test } from 'bun:test';
import { FINITE_VARIANTS, acceptsFiniteExpansion, acceptsFiniteValidation, acceptsFiniteLookup, acceptsReleasedContext, contextParameters, finiteScenarios } from './finite-model';

test('generated expectations use independent heap ancestry and finite set algebra', () => {
  const cases = finiteScenarios(0);
  expect(cases.find(c => c.id === '0-regex')!.expected).toEqual(['C00', 'C02', 'C04', 'C06', 'C08']);
  expect(cases.find(c => c.id === '0-generalizes')!.expected).toEqual(['C00', 'C02', 'C05', 'C11', 'C23']);
  expect(cases.find(c => c.id === '0-is-a')!.expected).toEqual(['C01', 'C03', 'C04', 'C07', 'C08', 'C09', 'C10', 'C15', 'C16', 'C17', 'C18', 'C19', 'C20', 'C21', 'C22']);
  expect(cases.find(c => c.id === '0-import-intersection')!.expected).toEqual(['C02', 'C04', 'C05', 'C07', 'C10', 'C14', 'C17', 'C19', 'C20', 'C22']);
  expect(cases.find(c => c.id === '0-diamond')!.expected).toEqual(['C01', 'C08', 'C11', 'C13', 'C16', 'C23']);
});

test('expansion activeOnly does not change validation membership of the supplied ValueSet', () => {
  const cases = finiteScenarios(0), active = cases.find(c => c.id === '0-inactive')!, compose = cases.find(c => c.id === '0-compose-inactive')!;
  expect(active.expected).not.toContain('C00'); expect(active.membership).toContain('C00');
  expect(compose.expected).not.toContain('C00'); expect(compose.membership).not.toContain('C00');
  expect(active.membership).toEqual(active.universe);
  expect(compose.membership).toEqual(compose.expected);
});

test('every bounded generated scenario has a genuine positive and complete unique source', () => {
  for (let seed = 0; seed < 64; seed++) {
    const cases = finiteScenarios(seed, 'same-trace'); expect(cases).toHaveLength(FINITE_VARIANTS.length);
    expect(new Set(cases.map(c => c.system)).size).toBe(cases.length);
    for (const scenario of cases) {
      expect(scenario.expected.length).toBeGreaterThan(0);
      expect(scenario.membership.length).toBeGreaterThan(0);
      expect(new Set(scenario.expected).size).toBe(scenario.expected.length);
      expect(scenario.expected.every(code => scenario.membership.includes(code))).toBe(true);
      const pending = [...scenario.resources[0].concept], codes: string[] = [];
      while (pending.length) { const node = pending.pop()!; codes.push(node.code); pending.push(...node.concept) }
      expect(codes.sort()).toEqual(scenario.universe);
      expect(scenario.resources.length).toBeLessThanOrEqual(4);
      expect(Buffer.byteLength(JSON.stringify(contextParameters(scenario)))).toBeLessThan(256 * 1024);
    }
  }
  for (const seed of [-1, 64, 0.5, NaN]) expect(() => finiteScenarios(seed)).toThrow();
});

test('finite response checks reject wrong totals, duplicates, labels, versions and unavailable context', () => {
  const scenario = finiteScenarios(0)[0];
  const contains = scenario.expected.map(code => ({ system: scenario.system, code, display: scenario.displays[code], version: '1' }));
  const valid = { resourceType: 'ValueSet', expansion: { total: contains.length, contains } };
  expect(acceptsFiniteExpansion(scenario, 200, valid)).toBe(true);
  const folded = finiteScenarios(1)[0];
  expect(acceptsFiniteExpansion(folded, 200, { resourceType: 'ValueSet', expansion: { total: folded.expected.length, contains: folded.expected.map(code => ({ system: folded.system, code: code.toLowerCase(), display: folded.displays[code], version: '1' })) } })).toBe(true);
  for (const entries of [[], [...contains.slice(1), contains[1]], contains.map(c => ({ ...c, display: 'wrong' })), contains.map(c => ({ ...c, version: '2' })), contains.map(c => ({ ...c, system: 'urn:other' })), contains.map(c => ({ ...c, contains: [] }))]) {
    expect(acceptsFiniteExpansion(scenario, 200, { ...valid, expansion: { ...valid.expansion, contains: entries } })).toBe(false);
  }
  const result = { resourceType: 'Parameters', parameter: [{ name: 'result', valueBoolean: false }] };
  expect(acceptsFiniteValidation(false, 200, result)).toBe(true);
  expect(acceptsFiniteValidation(true, 200, result)).toBe(false);
  for (const extra of [result.parameter[0], { name: 'x-caused-by-unknown-system', valueCanonical: scenario.system }, { name: 'version', valueString: '2' }]) {
    expect(acceptsFiniteValidation(false, 200, { ...result, parameter: [...result.parameter, extra] })).toBe(false);
  }
  expect(acceptsFiniteValidation(false, 404, result)).toBe(false);
  const lookup = { resourceType: 'Parameters', parameter: [{ name: 'display', valueString: scenario.displays[scenario.universe[0]] }, { name: 'version', valueString: '1' }] };
  expect(acceptsFiniteLookup(scenario, 200, lookup)).toBe(true);
  expect(acceptsFiniteLookup(scenario, 200, { ...lookup, parameter: [lookup.parameter[0]] })).toBe(false);
  expect(acceptsFiniteLookup(scenario, 200, { ...lookup, parameter: [...lookup.parameter, lookup.parameter[1]] })).toBe(false);
  expect(acceptsReleasedContext(404, { resourceType: 'OperationOutcome', issue: [{ severity: 'error', code: 'not-found' }] })).toBe(true);
  expect(acceptsReleasedContext(200, lookup)).toBe(false);
  expect(acceptsReleasedContext(404, '<html>')).toBe(false);
});
