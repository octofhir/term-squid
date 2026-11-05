import {expect, test} from 'bun:test';
import {booleanResult, comparisonCases} from './compare-api';

test('differential validation checks reject false positives and malformed result cardinality', () => {
  const yes = {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: true}]};
  expect(booleanResult(yes, true)).toBe(true);
  expect(booleanResult(yes, false)).toBe(false);
  expect(booleanResult({...yes, resourceType: 'OperationOutcome'}, true)).toBe(false);
  expect(booleanResult({...yes, parameter: [...yes.parameter, ...yes.parameter]}, true)).toBe(false);
  expect(booleanResult({resourceType: 'Parameters', parameter: [{name: 'result', valueString: 'true'}]}, true)).toBe(false);
});

test('comparison keeps all existing gates, reverse direction, errors and inline semantics', () => {
  const cases = comparisonCases();
  expect(new Set(cases.map(c => c.id)).size).toBe(cases.length);
  expect(cases.filter(c => c.id.startsWith('TS-')).length).toBe(17);
  for (const id of ['TS-CM02-1', 'translate-reverse-coding', 'unknown-version', 'vs-inline-excluded', 'cs-inline-missing']) {
    const test = cases.find(c => c.id === id)!;
    expect(test).toBeDefined();
    expect(test.accepts(200, {resourceType: 'Parameters', parameter: [{name: 'result', valueBoolean: true}]})).toBe(false);
  }
  const invalidUnit = cases.find(c => c.id === 'ucum-invalid')!;
  expect(invalidUnit.accepts(200, {resourceType: 'Parameters', parameter: [
    {name: 'result', valueBoolean: false},
    {name: 'x-caused-by-unknown-system', valueCanonical: 'http://unitsofmeasure.org'},
  ]})).toBe(false);
});


test('optional UCUM list comparisons preserve the original inventory and reject incomplete expansions', () => {
  const original=comparisonCases(), extended=comparisonCases(true);
  expect(extended.slice(0,original.length).map(c=>({id:c.id,request:c.request,expectation:c.expectation}))).toEqual(original.map(c=>({id:c.id,request:c.request,expectation:c.expectation})));
  const added=extended.slice(original.length);
  expect(added).toHaveLength(5);
  expect(new Set(extended.map(c=>c.id)).size).toBe(extended.length);
  for(const id of ['ucum-list-expand','ucum-list-exclude','ucum-list-finite-union']){
    const selected=added.find(c=>c.id===id)!;
    expect(selected.accepts(200,{resourceType:'ValueSet',expansion:{total:2,contains:[]}})).toBe(false);
    expect(selected.accepts(200,{resourceType:'ValueSet',expansion:{total:2,contains:[{system:'http://unitsofmeasure.org',code:'mg'},{system:'http://unitsofmeasure.org',code:'mg'}]}})).toBe(false);
  }
  const selected=added.find(c=>c.id==='ucum-list-expand')!;
  const correct={resourceType:'ValueSet',expansion:{total:2,contains:[{system:'http://unitsofmeasure.org',code:'mg',version:'2.2'},{system:'http://unitsofmeasure.org',code:'mg/dL',version:'2.2'}]}};
  expect(selected.accepts(200,correct)).toBe(true);
  expect(selected.accepts(200,{...correct,expansion:{...correct.expansion,contains:correct.expansion.contains.map(c=>({...c,version:'wrong'}))}})).toBe(false);
});
