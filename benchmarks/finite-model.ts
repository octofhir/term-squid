// An independent model of generated fixtures, not a parser for production FHIR inputs.
export const FINITE_VARIANTS = ['enumerated', 'import-intersection', 'diamond', 'is-a', 'generalizes', 'is-not-a', 'code-in', 'code-not-in', 'regex', 'property-equality', 'property-exists', 'inactive', 'compose-inactive'] as const;
export type FiniteScenario = {
  id: string; system: string; sensitive: boolean; universe: string[]; expected: string[]; membership: string[];
  displays: Record<string, string>; resources: any[]; valueSet: any; activeOnly: boolean;
};
const code = (n: number) => `C${String(n).padStart(2, '0')}`;
export function finiteScenarios(seed: number, epoch = 'unit'): FiniteScenario[] {
  if (!Number.isInteger(seed) || seed < 0 || seed > 63 || !/^[a-zA-Z0-9_.-]{1,80}$/.test(epoch)) throw new Error('Invalid generated fixture identity');
  const count = 24 + seed % 9, indexes = Array.from({ length: count }, (_, i) => i);
  const a = indexes.filter(i => (i + seed) % 3 !== 0), b = indexes.filter(i => (i * 3 + seed) % 5 < 3);
  const omitted = indexes.filter(i => (i + seed) % 7 === 0);
  const selected = new Set(Array.from({ length: 5 }, (_, i) => (seed * 7 + i * 3) % count));
  const anchor = 1 + seed % (count - 1), target = count - 1 - seed % (count - 1);
  // Integer heap ancestry is independent of the nested FHIR tree supplied below.
  const ancestors = (i: number) => { const result = new Set<number>(); for (let n = i; n > 0;) { n = Math.floor((n - 1) / 2); result.add(n) } return result };
  return FINITE_VARIANTS.map(variant => {
    const system = `urn:term-squid:finite-diff:${epoch}:${seed}:${variant}`, sensitive = seed % 2 === 0;
    const probe = (i: number) => sensitive ? code(i) : code(i).toLowerCase();
    const displays = Object.fromEntries(indexes.map(i => [code(i), `Generated ${seed} concept ${i}`]));
    const nodes = indexes.map(i => ({ code: code(i), display: displays[code(i)], property: [
      { code: 'group', valueCode: `g${(i + seed) % 3}` },
      ...((i + seed) % 2 === 0 ? [{ code: 'note', valueString: `tag-${seed}` }] : []),
      ...((i + seed) % 5 === 0 ? [{ code: 'inactive', valueBoolean: true }] : []),
    ], concept: [] as any[] }));
    for (let i = 1; i < count; i++) nodes[Math.floor((i - 1) / 2)].concept.push(nodes[i]);
    const cs = { resourceType: 'CodeSystem', url: system, version: '1', status: 'active', content: 'complete', caseSensitive: sensitive, hierarchyMeaning: 'is-a', property: [
      { code: 'group', type: 'code' }, { code: 'note', type: 'string' },
      { code: 'inactive', type: 'boolean', uri: 'http://hl7.org/fhir/concept-properties#inactive' },
    ], concept: [nodes[0]] };
    const explicit = (items: number[]) => ({ system, version: '1', concept: items.map(i => ({ code: probe(i) })) });
    const definition = (name: string, compose: any) => ({ resourceType: 'ValueSet', url: system + ':' + name, version: '1', status: 'active', compose });
    const left = definition('left', { include: [explicit(a)] }), right = definition('right', { include: [explicit(b)] });
    let resources: any[] = [cs], compose: any, expected: number[], activeOnly = false;
    switch (variant) {
      case 'enumerated': compose = { include: [explicit(a), explicit(b)], exclude: [explicit(omitted)] }; expected = indexes.filter(i => (a.includes(i) || b.includes(i)) && !omitted.includes(i)); break;
      case 'import-intersection': resources.push(left, right); compose = { include: [{ valueSet: [left.url, right.url] }] }; expected = a.filter(i => b.includes(i)); break;
      case 'diamond': {
        const middle = definition('middle', { include: [{ valueSet: [left.url] }, { valueSet: [right.url] }] });
        resources.push(left, right, middle); compose = { include: [{ valueSet: [middle.url] }, { valueSet: [left.url] }], exclude: [{ valueSet: [right.url] }] }; expected = a.filter(i => !b.includes(i)); break;
      }
      case 'is-a': compose = { include: [{ system, filter: [{ property: 'concept', op: 'is-a', value: probe(anchor) }] }] }; expected = indexes.filter(i => i === anchor || ancestors(i).has(anchor)); break;
      case 'generalizes': compose = { include: [{ system, filter: [{ property: 'concept', op: 'generalizes', value: probe(target) }] }] }; expected = indexes.filter(i => i === target || ancestors(target).has(i)); break;
      case 'is-not-a': compose = { include: [{ system, filter: [{ property: 'concept', op: 'is-not-a', value: probe(anchor) }] }] }; expected = indexes.filter(i => i !== anchor && !ancestors(i).has(anchor)); break;
      case 'code-in': case 'code-not-in': compose = { include: [{ system, filter: [{ property: 'code', op: variant === 'code-in' ? 'in' : 'not-in', value: [...selected].map(probe).join(',') }] }] }; expected = indexes.filter(i => selected.has(i) === (variant === 'code-in')); break;
      case 'regex': compose = { include: [{ system, filter: [{ property: 'code', op: 'regex', value: sensitive ? 'C0[02468]' : 'c0[02468]' }] }] }; expected = [0, 2, 4, 6, 8]; break;
      case 'property-equality': compose = { include: [{ system, filter: [{ property: 'group', op: '=', value: `g${seed % 3}` }] }] }; expected = indexes.filter(i => i % 3 === 0); break;
      case 'property-exists': { const present = seed % 2 === 0; compose = { include: [{ system, filter: [{ property: 'note', op: 'exists', value: String(present) }] }] }; expected = indexes.filter(i => ((i + seed) % 2 === 0) === present); break }
      case 'inactive': case 'compose-inactive': compose = { include: [{ system }], ...(variant === 'compose-inactive' ? { inactive: false } : {}) }; activeOnly = variant === 'inactive'; expected = indexes.filter(i => (i + seed) % 5 !== 0); break;
    }
    if (!expected.length) throw new Error('Every generated scenario needs a real positive control');
    return { id: `${seed}-${variant}`, system, sensitive, universe: indexes.map(code), expected: expected.map(code).sort(), membership: (activeOnly ? indexes : expected).map(code).sort(), displays, resources, valueSet: definition('root', compose), activeOnly };
  });
}
export function contextParameters(scenario: FiniteScenario, fields: any[] = []) {
  return { resourceType: 'Parameters', parameter: [
    ...scenario.resources.map(resource => ({ name: 'tx-resource', resource })),
    { name: 'valueSet', resource: scenario.valueSet }, ...fields,
  ] };
}
export function acceptsFiniteExpansion(scenario: FiniteScenario, status: number, body: any): boolean {
  const members = body?.expansion?.contains;
  const canonical = (value: unknown) => typeof value === 'string' ? (scenario.sensitive ? value : value.toUpperCase()) : undefined;
  return status === 200 && body?.resourceType === 'ValueSet' && body.expansion?.total === scenario.expected.length && Array.isArray(members) &&
    members.length === scenario.expected.length && JSON.stringify(members.map((entry: any) => canonical(entry?.code)).sort()) === JSON.stringify(scenario.expected) &&
    members.every((entry: any) => entry.system === scenario.system && entry.display === scenario.displays[canonical(entry.code)!] &&
      (entry.version === undefined || entry.version === '1') && entry.contains === undefined);
}
export function acceptsFiniteValidation(expected: boolean, status: number, body: any, system?: string): boolean {
  if (status !== 200 || body?.resourceType !== 'Parameters' || !Array.isArray(body.parameter)) return false;
  const result = body.parameter.filter((parameter: any) => parameter?.name === 'result');
  return result.length === 1 && result[0].valueBoolean === expected && !body.parameter.some((parameter: any) => parameter?.name === 'x-caused-by-unknown-system' || parameter?.name === 'version' && parameter.valueString !== '1' || system && parameter?.name === 'system' && parameter.valueUri !== system);
}

export function acceptsFiniteLookup(scenario: FiniteScenario, status: number, body: any): boolean {
  if (status !== 200 || body?.resourceType !== 'Parameters' || !Array.isArray(body.parameter)) return false;
  return ['display', 'version'].every(name => {
    const fields = body.parameter.filter((parameter: any) => parameter?.name === name);
    return fields.length === 1 && fields[0].valueString === (name === 'display' ? scenario.displays[scenario.universe[0]] : '1');
  });
}
export function acceptsReleasedContext(status: number, body: any): boolean {
  return status >= 400 && status < 500 && body?.resourceType === 'OperationOutcome' && Array.isArray(body.issue) && body.issue.length > 0;
}
