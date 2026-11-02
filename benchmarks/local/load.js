import http from 'k6/http';
import {check} from 'k6';
import {CASES, accepts} from './cases.js';

const test = CASES.find((c) => c.id === __ENV.TEST_ID);
if (!test) throw new Error(`Unknown local scenario: ${__ENV.TEST_ID}`);
export const options = {
  summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(95)', 'p(99)'],
  thresholds: {checks: ['rate==1'], http_req_failed: ['rate==0']},
};
export default function () {
  // Spread across the pool; use a per-VU sequence so repeated and missing codes occur.
  const p = test.sample(__ITER + (__VU - 1) * 137);
  const response = http.get(`${__ENV.BASE_URL}${p.request.path}`, {
    headers: {Accept: 'application/fhir+json'}, timeout: '5s', tags: {scenario_id: test.id},
  });
  let body;
  try { body = response.json(); } catch { body = null; }
  check(response, {'semantic response': (r) => accepts(p, r.status, body)});
}
export function handleSummary(data) {
  const m = data.metrics;
  const result = {
    test: test.id, vus: Number(__ENV.VUS), timestamp: new Date().toISOString(),
    duration: {p50: m.http_req_duration?.values.med, p95: m.http_req_duration?.values['p(95)'],
      p99: m.http_req_duration?.values['p(99)'], avg: m.http_req_duration?.values.avg},
    throughput: m.http_reqs?.values.rate, error_rate: m.http_req_failed?.values.rate,
    checks_pass: m.checks?.values.passes, checks_fail: m.checks?.values.fails,
    iterations: m.iterations?.values.count,
  };
  return {[__ENV.RESULT_FILE]: JSON.stringify(result, null, 2)};
}
