#!/usr/bin/env bun
// Local differential checks supplement, rather than replace, the pinned ecosystem suite.
import {createHash} from 'node:crypto';
import {mkdir, writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {parseArgs} from 'node:util';
import {targetUrl} from './bench';
import {CASES, SYSTEM, TARGET, VALUE_SET, MAP, VERSION, code, display} from './local/cases';

type Request = {path: string; method?: string; body?: unknown; headers?: Record<string, string>};
type Case = {id: string; expectation: string; request: Request; accepts: (status: number, body: any) => boolean};
const values = (body: any, name: string) => body?.resourceType === 'Parameters' && Array.isArray(body.parameter)
  ? body.parameter.filter((p: any) => p?.name === name) : [];
export const booleanResult = (body: any, expected: boolean) => {
  const result = values(body, 'result');
  return result.length === 1 && result[0].valueBoolean === expected;
};
const field = (name: string, key: string, expected: unknown) => (body: any) => {
  const result = values(body, name);
  return result.length === 1 && result[0][key] === expected;
};
const coding = (c = code(7)) => ({system: SYSTEM, version: VERSION, code: c});
const primitive = (name: string, type: string, value: unknown) => ({name, [type]: value});
const params = (...parameter: unknown[]) => ({resourceType: 'Parameters', parameter});
const query = (path: string, fields: Record<string, string>) => path + '?' + new URLSearchParams(fields);
const post = (id: string, path: string, body: unknown, expectation: string, check: (body: any) => boolean): Case =>
  ({id, expectation, request: {path, method: 'POST', body}, accepts: (status, actual) => status === 200 && check(actual)});
const bool = (expected: boolean) => (body: any) => booleanResult(body, expected);
const translation = (system: string, c: string) => (body: any) => booleanResult(body, true) &&
  values(body, 'match').some((match: any) => match.part?.some((p: any) =>
    p.name === 'concept' && p.valueCoding?.system === system && p.valueCoding?.code === c));
const INLINE = 'urn:term-squid:comparison:inline';
const inlineSystem = {resourceType: 'CodeSystem', url: INLINE, version: '1', status: 'active', content: 'complete',
  caseSensitive: true, date: '2020-01-01', concept: [{code: 'a', display: 'Alpha',
    designation: [{language: 'ru', value: 'Альфа'}]}, {code: 'b', display: 'Bravo'}]};
const inlineSet = {resourceType: 'ValueSet', status: 'active', compose: {
  include: [{system: SYSTEM, version: VERSION, concept: [{code: code(7)}, {code: code(8)}]}],
  exclude: [{system: SYSTEM, concept: [{code: code(8)}]}]}};
const inlineMap = {resourceType: 'ConceptMap', status: 'active', group: [{source: SYSTEM, target: TARGET,
  element: [{code: code(7), target: [{code: 'T00007', equivalence: 'equivalent'}]}]}]};

export function comparisonCases(includeUcumLists = false): Case[] {
  const result: Case[] = CASES.flatMap((test) => test.probes.map((probe, i) => ({
    id: `${test.id}-${i + 1}`, expectation: probe.expectation, request: probe.request,
    accepts: (status: number, body: any) => status === probe.status && probe.valid(body) === true,
  })));
  result.push(
    post('lookup-coding', '/CodeSystem/$lookup', params(primitive('coding', 'valueCoding', coding())),
      'Coding lookup preserves the selected business version', (b) => field('display', 'valueString', display(7))(b) && field('version', 'valueString', VERSION)(b)),
    post('cs-coding', '/CodeSystem/$validate-code', params(primitive('url', 'valueUri', SYSTEM), primitive('coding', 'valueCoding', coding())), 'Coding is valid', bool(true)),
    post('cs-codeable-concept', '/CodeSystem/$validate-code', params(primitive('url', 'valueUri', SYSTEM),
      primitive('codeableConcept', 'valueCodeableConcept', {coding: [coding('missing'), coding()]})), 'One valid CodeableConcept candidate succeeds', bool(true)),
    post('cs-wrong-display', '/CodeSystem/$validate-code', params(primitive('url', 'valueUri', SYSTEM),
      primitive('coding', 'valueCoding', {...coding(), display: 'Incorrect display'})), 'An incorrect display fails validation', bool(false)),
    post('cs-inline', '/CodeSystem/$validate-code', params({name: 'codeSystem', resource: inlineSystem}, primitive('code', 'valueCode', 'a')),
      'An inline CodeSystem validates its concept', bool(true)),
    post('cs-inline-missing', '/CodeSystem/$validate-code', params({name: 'codeSystem', resource: inlineSystem}, primitive('code', 'valueCode', 'missing')),
      'An inline complete CodeSystem rejects a missing code', bool(false)),
    post('cs-inline-language', '/CodeSystem/$validate-code', params({name: 'codeSystem', resource: inlineSystem}, primitive('code', 'valueCode', 'a'),
      primitive('display', 'valueString', 'Альфа'), primitive('displayLanguage', 'valueCode', 'ru')), 'A real Russian designation validates', bool(true)),
    post('vs-coding', '/ValueSet/$validate-code', params(primitive('url', 'valueUri', VALUE_SET), primitive('coding', 'valueCoding', coding())), 'Coding is a member', bool(true)),
    post('vs-codeable-concept', '/ValueSet/$validate-code', params(primitive('url', 'valueUri', VALUE_SET), primitive('codeableConcept', 'valueCodeableConcept', {coding: [coding(code(42)), coding()]})),
      'An included CodeableConcept candidate succeeds despite an excluded alternative', bool(true)),
    post('vs-inline', '/ValueSet/$validate-code', params({name: 'valueSet', resource: inlineSet}, primitive('coding', 'valueCoding', coding())), 'Inline composition includes the selected code', bool(true)),
    post('vs-inline-excluded', '/ValueSet/$validate-code', params({name: 'valueSet', resource: inlineSet}, primitive('coding', 'valueCoding', coding(code(8)))), 'Inline composition rejects the excluded code', bool(false)),
    post('expand-post', '/ValueSet/$expand', params(primitive('url', 'valueUri', VALUE_SET), primitive('count', 'valueInteger', 2), primitive('offset', 'valueInteger', 250)),
      'POST expansion retains exact total and page bounds', (b) => b?.resourceType === 'ValueSet' && b.expansion?.total === 255 && b.expansion?.offset === 250 && b.expansion?.contains?.length === 2),
    post('expand-inline', '/ValueSet/$expand', params({name: 'valueSet', resource: inlineSet}, primitive('count', 'valueInteger', 10)),
      'Inline expansion applies exclusions', (b) => b?.resourceType === 'ValueSet' && b.expansion?.total === 1 && b.expansion?.contains?.length === 1 && b.expansion.contains[0].system === SYSTEM && b.expansion.contains[0].code === code(7)),
    post('expand-filter', '/ValueSet/$expand', params(primitive('url', 'valueUri', VALUE_SET), primitive('filter', 'valueString', display(7)), primitive('count', 'valueInteger', 1000)),
      'Display filtering returns matching concepts', (b) => b?.resourceType === 'ValueSet' && b.expansion?.contains?.length > 0 && b.expansion.contains.every((c: any) => c.display?.includes(display(7)))),
    post('subsumes-codings', '/CodeSystem/$subsumes', params(primitive('codingA', 'valueCoding', coding('root')), primitive('codingB', 'valueCoding', coding())),
      'Coding subsumption returns the correct direction', field('outcome', 'valueCode', 'subsumes')),
    post('translate-coding', '/ConceptMap/$translate', params(primitive('url', 'valueUri', MAP), primitive('coding', 'valueCoding', coding()), primitive('targetsystem', 'valueUri', TARGET)),
      'Coding translation returns the mapped target', translation(TARGET, 'T00007')),
    post('translate-codeable-concept', '/ConceptMap/$translate', params(primitive('url', 'valueUri', MAP), primitive('codeableConcept', 'valueCodeableConcept', {coding: [coding('missing'), coding()]}), primitive('targetsystem', 'valueUri', TARGET)),
      'CodeableConcept translation finds the valid alternative', translation(TARGET, 'T00007')),
    post('translate-inline', '/ConceptMap/$translate', params({name: 'conceptMap', resource: inlineMap}, primitive('coding', 'valueCoding', coding()), primitive('targetsystem', 'valueUri', TARGET)),
      'An inline ConceptMap translates without publication', translation(TARGET, 'T00007')),
    post('translate-reverse-coding', '/ConceptMap/$translate', params(primitive('url', 'valueUri', MAP), primitive('coding', 'valueCoding', {system: TARGET, code: 'T00007'}), primitive('targetsystem', 'valueUri', SYSTEM), primitive('reverse', 'valueBoolean', true)),
      'Reverse Coding translation returns the original source code', translation(SYSTEM, code(7))),
  );
  result.push(post('cs-inline-coding', '/CodeSystem/$validate-code', params({name: 'codeSystem', resource: inlineSystem},
    primitive('coding', 'valueCoding', {system: INLINE, version: '1', code: 'a'})), 'Inline CodeSystem validation accepts explicit Coding context', bool(true)));
  result.push({id: 'expand-filter-get', expectation: 'GET text filtering restricts returned displays',
    request: {path: query('/ValueSet/$expand', {url: VALUE_SET, filter: display(7), count: '1000'})},
    accepts: (status, b) => status === 200 && b?.resourceType === 'ValueSet' && b.expansion?.contains?.length > 0 &&
      b.expansion.contains.every((c: any) => c.display?.includes(display(7)))});
  for (const unit of ['mg', 'mm[Hg]', 'mg/dL']) result.push(post(`ucum-${unit}`, '/CodeSystem/$validate-code',
    params(primitive('url', 'valueUri', 'http://unitsofmeasure.org'), primitive('code', 'valueCode', unit)), `UCUM validates ${unit}`, bool(true)));
  result.push(post('ucum-invalid', '/CodeSystem/$validate-code', params(primitive('url', 'valueUri', 'http://unitsofmeasure.org'), primitive('code', 'valueCode', 'not-a-ucum-unit')),
    'UCUM rejects an invalid grammar expression with known system context', (b) => bool(false)(b) &&
      values(b, 'x-caused-by-unknown-system').length === 0 && !values(b, 'issues').some((p: any) => p.resource?.issue?.some((i: any) => i.code === 'not-found'))));
  for (const [id, request] of [
    ['unknown-version', {path: query('/CodeSystem/$lookup', {system: SYSTEM, version: 'not-loaded', code: code(7)})}],
    ['missing-system', {path: query('/CodeSystem/$lookup', {code: code(7)})}],
  ] as const) result.push({id, request, expectation: 'Unavailable lookup context returns a FHIR error',
    accepts: (status, b) => status >= 400 && status < 500 && b?.resourceType === 'OperationOutcome' && b.issue?.length > 0});
  if (includeUcumLists) {
    const units = 'http://unitsofmeasure.org';
    const unitSet = {resourceType:'ValueSet',status:'active',compose:{include:[{system:units,concept:[{code:'mg'},{code:'mg/dL'}]}]}};
    const expansion = (codes: string[], mixed = false) => (body: any) => {
      if(body?.resourceType !== 'ValueSet' || body.expansion?.total !== codes.length + Number(mixed) || !Array.isArray(body.expansion?.contains)) return false;
      const entries=body.expansion.contains;
      const selected=entries.filter((entry:any)=>entry?.system===units);
      return entries.length===codes.length+Number(mixed) && JSON.stringify(selected.map((entry:any)=>entry.code).sort())===JSON.stringify([...codes].sort()) && selected.every((entry:any)=>entry.version===undefined||entry.version==='2.2') && (!mixed || entries.filter((entry:any)=>entry?.system===SYSTEM && entry.code===code(7)).length===1);
    };
    result.push(
      post('ucum-list-expand','/ValueSet/$expand',params({name:'valueSet',resource:unitSet}), 'An explicit finite unit list expands through the real grammar', expansion(['mg','mg/dL'])),
      post('ucum-list-member','/ValueSet/$validate-code',params({name:'valueSet',resource:unitSet},primitive('coding','valueCoding',{system:units,code:'mg'})), 'An explicitly selected UCUM code is a member',bool(true)),
      post('ucum-list-outside','/ValueSet/$validate-code',params({name:'valueSet',resource:unitSet},primitive('coding','valueCoding',{system:units,code:'kg'})), 'A valid unlisted unit is outside the finite ValueSet',bool(false)),
      post('ucum-list-exclude','/ValueSet/$expand',params({name:'valueSet',resource:{...unitSet,compose:{...unitSet.compose,exclude:[{system:units,concept:[{code:'mg'}]}]}}}), 'Explicit UCUM exclusions preserve the remaining code',expansion(['mg/dL'])),
      post('ucum-list-finite-union','/ValueSet/$expand',params({name:'valueSet',resource:{...unitSet,compose:{include:[...unitSet.compose.include,{system:SYSTEM,concept:[{code:code(7)}]}]}}}), 'UCUM lists compose with the same installed finite fixture',expansion(['mg','mg/dL'],true)),
    );
  }
  return result;
}

async function main() {
  const {values: options} = parseArgs({options: {left: {type: 'string'}, right: {type: 'string'}, output: {type: 'string'}, 'ucum-lists': {type: 'boolean'}}});
  const targets = {termSquid: targetUrl(options.left || 'http://127.0.0.1:18081/r4'),
    fhirsmith: targetUrl(options.right || 'http://127.0.0.1:17003/tx/r4')};
  const folder = resolve(options.output || '.benchmarks/api-comparison-' + new Date().toISOString().replace(/[:.]/g, '-'));
  const root = resolve('.benchmarks') + '/';
  if (!folder.startsWith(root)) throw new Error('Write raw responses under .benchmarks/.');
  await mkdir(folder, {recursive: true});
  const cases = comparisonCases(options['ucum-lists']);
  const results = [];
  for (const test of cases) {
    const responses: Record<string, unknown> = {};
    for (const [name, base] of Object.entries(targets)) {
      try {
        const response = await fetch(base + test.request.path, {method: test.request.method || 'GET',
          headers: {Accept: 'application/fhir+json', ...(test.request.body ? {'Content-Type': 'application/fhir+json'} : {}), ...test.request.headers},
          body: test.request.body ? JSON.stringify(test.request.body) : undefined, signal: AbortSignal.timeout(15_000)});
        const text = await response.text();
        let body: any;
        try { body = JSON.parse(text); } catch { body = text; }
        let passed = false;
        try { passed = test.accepts(response.status, body) === true; } catch { /* Malformed responses fail the check. */ }
        responses[name] = {status: response.status, contentType: response.headers.get('content-type'), passed, body};
      } catch (error) { responses[name] = {passed: false, error: String(error)}; }
    }
    results.push({id: test.id, expectation: test.expectation, request: test.request, responses});
    console.log(test.id, ...Object.entries(responses).map(([name, value]) => `${name}=${(value as any).passed ? 'pass' : 'FAIL'}`));
  }
  const summary = Object.fromEntries(Object.keys(targets).map(name => [name, {passed: results.filter(r => (r.responses[name] as any).passed).length, total: results.length}]));
  await writeFile(folder + '/responses.json', JSON.stringify({targets, profiles: options['ucum-lists'] ? ['default', 'ucum-lists'] : ['default'], caseFingerprint: createHash('sha256').update(JSON.stringify(cases.map(c => ({id:c.id,request:c.request,expectation:c.expectation})))).digest('hex'),
    generatedAt: new Date().toISOString(), summary, results}, null, 2) + '\n', {flag: 'wx'});
  console.log(JSON.stringify(summary));
  // Reference differences remain evidence; regressions in our declared supported cases fail the run.
  if (summary.termSquid.passed !== summary.termSquid.total) process.exitCode = 1;
}
if (import.meta.main) main().catch(error => {console.error(error); process.exitCode = 1;});
