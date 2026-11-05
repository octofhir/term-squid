import http from 'k6/http';
import {check} from 'k6';
import {Counter, Trend} from 'k6/metrics';
import execution from 'k6/execution';
import {CASES, accepts} from '../local/cases.js';
import {PLACEHOLDER, contextAccepted} from './cases.js';
const template = open(__ENV.CONTEXT_FIXTURE);
const lookupCase = CASES.find(c => c.id === 'TS-LK01');
const lookupRequests = new Counter('lookup_requests'), contextRequests = new Counter('context_requests');
const lookupLatency = new Trend('lookup_latency', true), contextLatency = new Trend('context_latency', true);
const boundaryCallbacks = new Counter('boundary_callbacks');
const rate = Number(__ENV.CONTEXT_RATE);
const seconds = Number(__ENV.MIXED_DURATION.slice(0, -1));
export const options = {
  summaryTrendStats: ['avg','min','med','max','p(95)','p(99)'],
  thresholds: {checks:['rate==1'],http_req_failed:['rate==0'],dropped_iterations:['count==0']},
  scenarios: {
    lookup: {executor:'constant-arrival-rate',rate:100,timeUnit:'1s',duration:__ENV.MIXED_DURATION,preAllocatedVUs:20,maxVUs:40,exec:'lookup',gracefulStop:'15s'},
    ...(rate ? {context:{executor:'constant-arrival-rate',rate,timeUnit:'1s',duration:__ENV.MIXED_DURATION,preAllocatedVUs:16,maxVUs:32,exec:'context',gracefulStop:'15s'}} : {}),
  },
};
export function lookup() {
  // An executor may call at the end boundary. Generate exactly the specified arrivals;
  // no HTTP response is discarded and the independent admission assertions stay exact.
  if (execution.scenario.iterationInTest >= seconds * 100) { boundaryCallbacks.add(1); return; }
  const probe=lookupCase.sample(execution.scenario.iterationInTest);
  const response=http.get(__ENV.BASE_URL+probe.request.path,{headers:{Accept:'application/fhir+json'},timeout:'5s',tags:{operation:'lookup'}});
  let body;try{body=response.json()}catch{body=null}
  lookupRequests.add(1);lookupLatency.add(response.timings.duration);
  check(response,{'unchanged lookup semantics':r=>accepts(probe,r.status,body)});
}
export function context() {
  if (execution.scenario.iterationInTest >= seconds * rate) { boundaryCallbacks.add(1); return; }
  // Each context has a unique canonical; repeated registry URL hits are not the workload.
  const system=`urn:term-squid:mixed:${__ENV.CONTEXT_EPOCH}:${execution.scenario.iterationInTest}`;
  const response=http.post(__ENV.BASE_URL+'/ValueSet/$expand',template.split(PLACEHOLDER).join(system),{headers:{Accept:'application/fhir+json','Content-Type':'application/fhir+json'},timeout:'10s',tags:{operation:'context'}});
  let body;try{body=response.json()}catch{body=null}
  contextRequests.add(1);contextLatency.add(response.timings.duration);
  check(response,{'complete cold expansion semantics':r=>contextAccepted(r.status,body,system)});
}
export function handleSummary(data) {
  const metric=name=>data.metrics[name]?.values;
  const trend=name=>{const values=metric(name);return values?{p50:values.med,p95:values['p(95)'],p99:values['p(99)'],avg:values.avg,max:values.max}:null};
  return {[__ENV.RESULT_FILE]:JSON.stringify({timestamp:new Date().toISOString(),lookup:{requests:metric('lookup_requests')?.count||0,latency:trend('lookup_latency')},context:{requests:metric('context_requests')?.count||0,latency:trend('context_latency')},checks_pass:metric('checks')?.passes||0,checks_fail:metric('checks')?.fails||0,error_rate:metric('http_req_failed')?.rate,dropped_iterations:metric('dropped_iterations')?.count||0,boundary_callbacks:metric('boundary_callbacks')?.count||0,iterations:metric('iterations')?.count||0},null,2)};
}
