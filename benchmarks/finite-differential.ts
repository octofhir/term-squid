#!/usr/bin/env bun
// Sequential local semantic checks; these are not load windows or a conformance score.
import { parseArgs } from 'node:util';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, resolve, sep } from 'node:path';
import { targetUrl, sourceFingerprint } from './bench';
import { acceptsFiniteExpansion, acceptsFiniteValidation, acceptsFiniteLookup, acceptsReleasedContext, contextParameters, finiteScenarios } from './finite-model';

async function main() {
  const { values } = parseArgs({ options: { left: { type: 'string' }, right: { type: 'string' }, output: { type: 'string' }, seeds: { type: 'string' }, 'left-revision': { type: 'string' }, 'right-revision': { type: 'string' } } });
  const endpoints = [targetUrl(values.left || 'http://127.0.0.1:18081/r4'), targetUrl(values.right || 'http://127.0.0.1:17003/tx/r4')];
  const seeds = Number(values.seeds || '16'); if (!Number.isInteger(seeds) || seeds < 1 || seeds > 64) throw new Error('Use 1..64 generated seeds');
  const out = resolve(values.output || `.benchmarks/finite-differential-${Date.now()}`), epoch = basename(out);
  if (!out.startsWith(resolve('.benchmarks') + sep) || !/^[a-zA-Z0-9_.-]{1,80}$/.test(epoch)) throw new Error('Use a fresh named directory under .benchmarks/');
  await mkdir(dirname(out), { recursive: true }); await mkdir(out);
  const files = ['benchmarks/finite-differential.ts', 'benchmarks/finite-model.ts', 'benchmarks/bench.ts'];
  const sources = await Promise.all(files.map(async file => [file, await readFile(file, 'utf8')] as const));
  const scenarios = Array.from({ length: seeds }, (_, seed) => finiteScenarios(seed, epoch)).flat();
  const rows: any[] = [], manifest: any = { kind: 'generated-finite-semantic-differential', scoredComposite: false, startedAt: new Date().toISOString(), endpoints, revisions: [values['left-revision'] || null, values['right-revision'] || null], seeds, scenarios: scenarios.length, harness: { files, sha256: sourceFingerprint(sources) }, status: 'running' };
  await writeFile(resolve(out, 'fixtures.json'), JSON.stringify(scenarios, null, 2), { flag: 'wx' });
  try {
    for (const base of endpoints) {
      const response = await fetch(base + '/metadata', { signal: AbortSignal.timeout(10_000) }), body: any = await response.json();
      if (response.status !== 200 || body.resourceType !== 'CapabilityStatement' || body.fhirVersion !== '4.0.1') throw new Error('Both participants need the verified R4 adapter');
    }
    for (const [side, base] of endpoints.entries()) for (const scenario of scenarios) {
      const member = scenario.membership[0], outside = scenario.universe.find(code => !scenario.membership.includes(code)) || 'ABSENT';
      const requests = [
        { id: 'lookup', method: 'POST', path: '/CodeSystem/$lookup', body: { resourceType: 'Parameters', parameter: [{ name: 'tx-resource', resource: scenario.resources[0] }, { name: 'system', valueUri: scenario.system }, { name: 'version', valueString: '1' }, { name: 'code', valueCode: scenario.universe[0] }] }, check: (status: number, body: any) => acceptsFiniteLookup(scenario, status, body) },
        { id: 'expand', method: 'POST', path: '/ValueSet/$expand', body: contextParameters(scenario, [{ name: 'count', valueInteger: 100 }, { name: 'excludeNested', valueBoolean: true }, ...(scenario.activeOnly ? [{ name: 'activeOnly', valueBoolean: true }] : [])]), check: (status: number, body: any) => acceptsFiniteExpansion(scenario, status, body) },
        ...[['member', member, true], ['outside', outside, false], ['unknown', 'ABSENT', false]].map(([id, code, expected]) => ({
          id, method: 'POST', path: '/ValueSet/$validate-code', body: contextParameters(scenario, [{ name: 'coding', valueCoding: { system: scenario.system, version: '1', code: scenario.sensitive ? code : String(code).toLowerCase() } }]),
          check: (status: number, body: any) => acceptsFiniteValidation(Boolean(expected), status, body, scenario.system),
        })),
        { id: 'released-context', method: 'GET', path: '/CodeSystem/$lookup?' + new URLSearchParams({ system: scenario.system, version: '1', code: scenario.universe[0] }), body: undefined, check: acceptsReleasedContext },
      ];
      for (const request of requests) {
        let status = 0, actual: any = null, error: string | undefined;
        try { const response = await fetch(base + request.path, { method: request.method, headers: { Accept: 'application/fhir+json', 'Content-Type': 'application/fhir+json' }, body: JSON.stringify(request.body), signal: AbortSignal.timeout(15_000) }); status = response.status; actual = await response.json() }
        catch (failure) { error = String(failure) }
        rows.push({ side, scenario: scenario.id, check: request.id, request: { method: request.method, path: request.path, body: request.body }, status, actual, error, passed: request.check(status, actual) });
      }
    }
    manifest.participants = endpoints.map((base, side) => {
      const selected = rows.filter(row => row.side === side);
      const accepted = scenarios.filter(scenario => selected.filter(row => row.scenario === scenario.id).length === 6 && selected.filter(row => row.scenario === scenario.id).every(row => row.passed));
      return { base, checkedResponses: selected.length, passedChecks: selected.filter(row => row.passed).length, passedScenarios: accepted.length, failedScenarios: scenarios.length - accepted.length };
    });
    manifest.status = manifest.participants[0].failedScenarios === 0 ? 'passed' : 'failed';
    console.log(JSON.stringify(manifest.participants));
    if (manifest.status !== 'passed') process.exitCode = 1;
  } catch (error) {
    manifest.status = 'failed'; manifest.error = String(error); throw error;
  } finally {
    manifest.finishedAt = new Date().toISOString();
    await writeFile(resolve(out, 'responses.json'), JSON.stringify(rows, null, 2), { flag: 'wx' });
    await writeFile(resolve(out, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  }
}
if (import.meta.main) main().catch(error => { console.error(error); process.exitCode = 1 });
