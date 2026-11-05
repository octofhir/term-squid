import {test,expect} from 'bun:test';
import {contextFixture,contextAccepted,PLACEHOLDER} from './mixed/cases.js';
import {validMixedResult} from './context-mixed';
test('cold generated context stays bounded and admits only the exact complete page',()=>{
  const fixture=contextFixture();const source=fixture.parameter[0].resource;
  const pending=source.concept.map(c=>[c,1] as const);let count=0,maxDepth=0;
  while(pending.length){const [node,depth]=pending.pop()!;count++;maxDepth=Math.max(maxDepth,depth);for(const child of node.concept||[])pending.push([child,depth+1] as const)}
  expect(count).toBe(3000);expect(maxDepth).toBe(30);expect(JSON.stringify(source).length).toBeLessThan(256*1024);
  const correct={resourceType:'ValueSet',expansion:{total:3000,contains:[{system:PLACEHOLDER,version:'1',code:'N00000',display:'Generated node 0'}]}};
  expect(contextAccepted(200,correct,PLACEHOLDER)).toBe(true);
  for(const body of [{...correct,expansion:{...correct.expansion,total:1}},{...correct,expansion:{...correct.expansion,contains:[]}},{...correct,expansion:{...correct.expansion,contains:[{...correct.expansion.contains[0],system:'urn:other'}]}}])expect(contextAccepted(200,body,PLACEHOLDER)).toBe(false);
  expect(contextAccepted(503,correct,PLACEHOLDER)).toBe(false);
  expect(()=>contextFixture(3001)).toThrow();expect(()=>contextFixture(3,33)).toThrow();
});
test('mixed admission rejects dropped arrivals, semantic errors and missing streams',()=>{
  const latency={p50:1,p95:2,p99:3,avg:1,max:4};
  const correct={lookup:{requests:3000,latency},context:{requests:240,latency},checks_pass:3240,checks_fail:0,error_rate:0,dropped_iterations:0};
  expect(validMixedResult(correct,30,8)).toBe(true);
  for(const change of [{dropped_iterations:1},{checks_fail:1},{error_rate:.01},{lookup:{requests:2999,latency}},{context:{requests:239,latency}},{checks_pass:3239},{context:{requests:240,latency:{...latency,p99:NaN}}}])expect(validMixedResult({...correct,...change},30,8)).toBe(false);
});
