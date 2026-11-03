// This synthetic dataset is intentionally separate from the upstream clinical pools.
export const SYSTEM = 'http://example.org/term-squid-benchmark/codes';
export const TARGET = 'http://example.org/term-squid-benchmark/targets';
export const VALUE_SET = 'http://example.org/term-squid-benchmark/subset';
export const MAP = 'http://example.org/term-squid-benchmark/map';
export const VERSION = '1.0.0';
export const CONCEPT_COUNT = 10000;
export const code = (n) => `C${String(n).padStart(5, '0')}`;
export const targetCode = (n) => `T${String(n).padStart(5, '0')}`;
export const display = (n) => `Benchmark concept ${n}`;

const query = (path, params) => ({
  path: `${path}?${Object.entries(params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&')}`,
});
const parameter = (body, name) => body?.resourceType === 'Parameters'
  ? body.parameter?.find((p) => p.name === name) : undefined;
const bool = (body) => body?.resourceType === 'Parameters' ? parameter(body, 'result')?.valueBoolean : undefined;
const translation = (body, system, c) => bool(body) === true &&
  body.parameter.some((p) => p.name === 'match' && p.part?.some((part) =>
    part.name === 'concept' && part.valueCoding?.system === system && part.valueCoding?.code === c));
const lookup = (n) => query('/CodeSystem/$lookup', {system: SYSTEM, version: VERSION, code: code(n)});
const csValidate = (c) => query('/CodeSystem/$validate-code', {system: SYSTEM, version: VERSION, code: c});
const vsValidate = (n) => query('/ValueSet/$validate-code', {url: VALUE_SET, system: SYSTEM, code: code(n)});
const translate = (n, reverse) => query('/ConceptMap/$translate', {
  url: MAP, system: reverse ? TARGET : SYSTEM, code: reverse ? targetCode(n) : code(n),
  targetsystem: reverse ? SYSTEM : TARGET, reverse: String(reverse),
});
const probe = (request, valid, expectation, status = 200) => ({request, valid, expectation, status});
const expansion = (body, offset, length) => {
  const entries = body?.expansion?.contains;
  return body?.resourceType === 'ValueSet' && body.expansion.total === 255 &&
    (body.expansion.offset ?? 0) === offset && entries?.length === length &&
    new Set(entries.map((c) => c.code)).size === length && entries.every((c) =>
      c.system === SYSTEM && /^C\d{5}$/.test(c.code) && Number(c.code.slice(1)) < 256 && c.code !== code(42));
};

export const CASES = [
  {
    id: 'TS-CS01', name: 'CodeSystem validation, existing and missing codes',
    probes: [probe(csValidate(code(7)), (b) => bool(b) === true, 'existing code is valid'),
      probe(csValidate('missing'), (b) => bool(b) === false, 'missing code is invalid')],
    sample(i) {
      return i % 5 === 0
        ? probe(csValidate('missing'), (b) => bool(b) === false, 'missing code is invalid')
        : probe(csValidate(code(i % CONCEPT_COUNT)), (b) => bool(b) === true, 'existing code is valid');
    },
  },
  {
    id: 'TS-LK01', name: 'Lookup with the requested terminology version',
    probes: [probe(lookup(7), (b) => b?.resourceType === 'Parameters' &&
      parameter(b, 'display')?.valueString === display(7) && parameter(b, 'version')?.valueString === VERSION,
    'lookup returns display and terminology version')],
    sample(i) { const n = i % CONCEPT_COUNT; return probe(lookup(n), (b) =>
      parameter(b, 'display')?.valueString === display(n) && parameter(b, 'version')?.valueString === VERSION,
    'lookup returns display and terminology version'); },
  },
  {
    id: 'TS-VC01', name: 'ValueSet membership, exclusions and outside codes',
    probes: [probe(vsValidate(7), (b) => bool(b) === true, 'included code is valid'),
      probe(vsValidate(42), (b) => bool(b) === false, 'excluded code is invalid'),
      probe(vsValidate(999), (b) => bool(b) === false, 'code outside the ValueSet is invalid')],
    sample(i) {
      // Stable mix at every rate: 40% included, 20% excluded, 20% outside, 20% missing.
      let n;
      switch (i % 5) {
        case 0: n = 10000 + i % 10000; break;
        case 2: n = 42; break;
        case 3: n = 256 + i % (CONCEPT_COUNT - 256); break;
        default: n = (i * 17) % 256; if (n === 42) n = 43;
      }
      return probe(vsValidate(n), (b) => bool(b) === (n < 256 && n !== 42),
        'ValueSet membership with changing, excluded, outside and missing codes');
    },
  },
  {
    id: 'TS-EX01', name: 'ValueSet expansion with exclusion and pagination',
    probes: [probe(query('/ValueSet/$expand', {url: VALUE_SET, count: 50, offset: 0}), (b) =>
      expansion(b, 0, 50), '255 members, first page has 50 codes without the excluded code'),
    probe(query('/ValueSet/$expand', {url: VALUE_SET, count: 50, offset: 250}), (b) =>
      expansion(b, 250, 5), 'last page has 5 members'),
    probe(query('/ValueSet/$expand', {url: VALUE_SET, count: 1000, offset: 0}), (b) =>
      expansion(b, 0, 255), 'full expansion contains exactly the expected set')],
    sample(i) { const offset = [0, 50, 250][i % 3]; return probe(query('/ValueSet/$expand', {
      url: VALUE_SET, count: 50, offset,
    }), (b) => expansion(b, offset, offset === 250 ? 5 : 50), 'paginated expansion'); },
  },
  {
    id: 'TS-SS01', name: 'Hierarchy subsumption',
    probes: [probe(query('/CodeSystem/$subsumes', {system: SYSTEM, version: VERSION, codeA: 'root', codeB: code(7)}),
      (b) => parameter(b, 'outcome')?.valueCode === 'subsumes', 'root subsumes child')],
    sample(i) { return probe(query('/CodeSystem/$subsumes', {
      system: SYSTEM, version: VERSION, codeA: 'root', codeB: code(i % CONCEPT_COUNT),
    }), (b) => parameter(b, 'outcome')?.valueCode === 'subsumes', 'root subsumes child'); },
  },
  ...[false, true].map((reverse) => ({
    id: reverse ? 'TS-CM02' : 'TS-CM01', name: reverse ? 'Reverse translation' : 'Forward translation',
    probes: [probe(translate(7, reverse), (b) => translation(b, reverse ? SYSTEM : TARGET,
      reverse ? code(7) : targetCode(7)), 'translation returns the mapped code and system'),
      probe(query('/ConceptMap/$translate', {url: MAP, system: reverse ? TARGET : SYSTEM,
        code: 'missing', targetsystem: reverse ? SYSTEM : TARGET, reverse: String(reverse)}),
      (b) => bool(b) === false, 'missing mapping returns result=false')],
    sample(i) {
      if (i % 5 === 0) return probe(query('/ConceptMap/$translate', {
        url: MAP, system: reverse ? TARGET : SYSTEM, code: `missing-${i % 10000}`,
        targetsystem: reverse ? SYSTEM : TARGET, reverse: String(reverse),
      }), (b) => bool(b) === false, 'missing mapping returns result=false');
      const n = i % 128;
      return probe(translate(n, reverse), (b) => translation(b,
        reverse ? SYSTEM : TARGET, reverse ? code(n) : targetCode(n)), 'translation returns the mapped code and system');
    },
  })),
];

export function accepts(probe, status, body) {
  try { return status === probe.status && probe.valid(body) === true; } catch { return false; }
}

export function resources() {
  return [
    {resourceType: 'CodeSystem', id: 'benchmark-codes', url: SYSTEM, version: VERSION,
      name: 'TermSquidBenchmarkCodes', status: 'active', content: 'complete', caseSensitive: true,
      hierarchyMeaning: 'is-a', property: [{code: 'parent', uri: 'http://hl7.org/fhir/concept-properties#parent', type: 'code'}],
      concept: [{code: 'root', display: 'Benchmark root'}, ...Array.from({length: CONCEPT_COUNT}, (_, i) => ({
        code: code(i), display: display(i), property: [{code: 'parent', valueCode: 'root'}],
      }))]},
    {resourceType: 'CodeSystem', id: 'benchmark-targets', url: TARGET, version: VERSION,
      name: 'TermSquidBenchmarkTargets', status: 'active', content: 'complete', caseSensitive: true,
      concept: Array.from({length: 128}, (_, i) => ({code: targetCode(i), display: `Target ${i}`}))},
    {resourceType: 'ValueSet', id: 'benchmark-subset', url: VALUE_SET, version: VERSION,
      name: 'TermSquidBenchmarkSubset', status: 'active', compose: {
        include: [{system: SYSTEM, version: VERSION, concept: Array.from({length: 256}, (_, i) => ({code: code(i)}))}],
        exclude: [{system: SYSTEM, version: VERSION, concept: [{code: code(42)}]}],
      }},
    {resourceType: 'ConceptMap', id: 'benchmark-map', url: MAP, version: VERSION,
      name: 'TermSquidBenchmarkMap', status: 'active', group: [{source: SYSTEM, target: TARGET,
        element: Array.from({length: 128}, (_, i) => ({code: code(i), target: [{
          code: targetCode(i), display: `Target ${i}`, equivalence: 'equivalent',
        }]}))}]},
  ];
}
