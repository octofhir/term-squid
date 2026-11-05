#!/usr/bin/env bun
// A separate fixed-arrival diagnostic; it never changes the original benchmark/scorer.
import {parseArgs} from 'node:util';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {targetUrl, sourceFingerprint} from './bench';
import {CASES, accepts} from './local/cases.js';
import {contextFixture, contextAccepted, PLACEHOLDER} from './mixed/cases.js';

export function validMixedResult(result: any, seconds: number, rate: number) {
  const finite=(value:any)=>value && ['p50','p95','p99','avg','max'].every(k=>Number.isFinite(value[k])&&value[k]>=0);
  return result?.lookup?.requests===seconds*100 && result?.context?.requests===seconds*rate && result.checks_fail===0 && result.error_rate===0 && result.dropped_iterations===0 && result.checks_pass===seconds*(100+rate) && finite(result.lookup.latency) && (rate===0 || finite(result.context.latency));
}
async function main() {
  const {values}=parseArgs({options:{url:{type:'string'},name:{type:'string'},'run-id':{type:'string'},'heavy-rate':{type:'string'},duration:{type:'string'},'server-revision':{type:'string'}}});
  const base=targetUrl(values.url||'http://127.0.0.1:18081/r4');
  const identifier=(value:string)=>{if(!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,79}$/.test(value))throw new Error('Invalid local run identity');return value};
  const name=identifier(values.name||'term-squid'), runId=identifier(values['run-id']||'mixed-'+Date.now());
  const duration=values.duration||'30s';if(!/^(?:[1-9]|[1-5][0-9]|60)s$/.test(duration))throw new Error('Mixed duration must be 1..60 seconds');
  const seconds=Number(duration.slice(0,-1)), rate=Number(values['heavy-rate']||'8');
  if(!Number.isInteger(rate)||rate<0||rate>32)throw new Error('Cold context arrival rate must be 0..32 per second');
  const out=resolve('.benchmarks/context-mixed',runId,name);await mkdir(resolve(out,'..'),{recursive:true});await mkdir(out);
  const fixture=JSON.stringify(contextFixture()),fixturePath=resolve(out,'context-fixture.json');await writeFile(fixturePath,fixture,{flag:'wx'});
  const files=['benchmarks/context-mixed.ts','benchmarks/mixed/cases.js','benchmarks/mixed/load.js','benchmarks/local/cases.js','benchmarks/bench.ts'];
  const sources=await Promise.all(files.map(async file=>[file,await readFile(file,'utf8')] as const));
  const manifest:any={runId,server:name,baseUrl:base,serverRevision:values['server-revision']||null,kind:'fixed-arrival-generated-context-diagnostic',scoredComposite:false,protocol:{duration,lookupRate:100,contextRate:rate,lookupMaxVus:40,contextMaxVus:32},fixtureSha256:createHash('sha256').update(fixture).digest('hex'),harness:{files,sha256:sourceFingerprint(sources)},startedAt:new Date().toISOString(),status:'running'};
  try {
    const metadataResponse=await fetch(base+'/metadata',{signal:AbortSignal.timeout(10_000)});const metadata=await metadataResponse.json();if(metadataResponse.status!==200||metadata?.resourceType!=='CapabilityStatement'||metadata.fhirVersion!=='4.0.1')throw new Error('Requires the R4 adapter');manifest.serverSoftware=metadata.software;
    const probe=CASES.find(c=>c.id==='TS-LK01')!.probes[0], lookup=await fetch(base+probe.request.path,{signal:AbortSignal.timeout(10_000)}), lookupBody=await lookup.json();
    if(!accepts(probe,lookup.status,lookupBody))throw new Error('Unchanged lookup preflight failed');
    // Warm the exact measured code trace on both participants; VU scheduling cannot
    // choose different codes or give one participant an additional per-code cache pass.
    const selected=CASES.find(c=>c.id==='TS-LK01')!;let next=0,warmed=0;
    await Promise.all(Array.from({length:20},async()=>{
      while(next<seconds*100){
        const probe=selected.sample(next++),response=await fetch(base+probe.request.path,{signal:AbortSignal.timeout(10_000)}),body=await response.json();
        if(!accepts(probe,response.status,body))throw new Error('Matched lookup trace warmup failed');
        warmed++;
      }
    }));
    if(warmed!==seconds*100)throw new Error('Incomplete matched lookup warmup');
    manifest.warmup={lookupRequests:warmed,concurrency:20,trace:'global scenario iteration index'};
    if(rate){const system='urn:term-squid:mixed:'+runId+':preflight';const response=await fetch(base+'/ValueSet/$expand',{method:'POST',headers:{'Content-Type':'application/fhir+json'},body:fixture.split(PLACEHOLDER).join(system),signal:AbortSignal.timeout(15_000)});const body=await response.json();await writeFile(resolve(out,'preflight.json'),JSON.stringify({status:response.status,body},null,2));if(!contextAccepted(response.status,body,system))throw new Error('Cold context semantic preflight failed');}
    const child=Bun.spawn(['k6','run','--quiet','--no-usage-report','benchmarks/mixed/load.js'],{env:{...globalThis.process.env,BASE_URL:base,CONTEXT_FIXTURE:fixturePath,CONTEXT_RATE:String(rate),CONTEXT_EPOCH:runId,MIXED_DURATION:duration,RESULT_FILE:resolve(out,'measurement.json')},stdout:'pipe',stderr:'pipe'});
    const [stdout,stderr,exitCode]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);await writeFile(resolve(out,'k6.log'),stdout+stderr);
    const result=JSON.parse(await readFile(resolve(out,'measurement.json'),'utf8'));manifest.measurement=result;
    if(exitCode!==0||!validMixedResult(result,seconds,rate))throw new Error('Mixed diagnostic failed semantic, arrival or latency-data admission; raw result retained, unscored');
    manifest.status='passed';console.log(JSON.stringify({server:name,rate,summary:result}));
  }catch(error){manifest.status='failed';manifest.error=String(error);throw error;}
  finally{manifest.finishedAt=new Date().toISOString();await writeFile(resolve(out,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');}
}
if(import.meta.main)main().catch(error=>{console.error(error);process.exitCode=1});
