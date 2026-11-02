import {writeFileSync} from 'node:fs';
const base = process.env.PROFILE_URL || 'http://127.0.0.1:18082';
if (!['127.0.0.1', 'localhost', '[::1]'].includes(new URL(base).hostname)) throw new Error('Allocation profiling requires a loopback server');
if (!process.argv[2]) throw new Error('Usage: bun benchmarks/profile-allocations.ts .benchmarks/profiles/allocations.json');
const probe = async () => (await fetch(base+'/__allocation-probe')).json();
const request = async (i: number) => {
  const membership = process.env.PROFILE_OPERATION === 'membership';
  const code = i % 5 === 0 ? 'missing' : 'C'+String(i%10000).padStart(5,'0');
  const r = await fetch(base+(membership ? '/r4/ValueSet/$validate-code?url=http%3A%2F%2Fexample.org%2Fterm-squid-benchmark%2Fsubset&system=http%3A%2F%2Fexample.org%2Fterm-squid-benchmark%2Fcodes&code=' : '/r4/CodeSystem/$validate-code?system=http%3A%2F%2Fexample.org%2Fterm-squid-benchmark%2Fcodes&version=1.0.0&code=')+code);
  const b = await r.json();
  if (r.status !== 200 || b.parameter?.find((p:any)=>p.name==='result')?.valueBoolean !== (i%5!==0 && (!membership || (i%10000 < 256 && i%10000 !== 42)))) throw new Error('semantic failure');
};
for (let i=0;i<20;i++) await request(i);
const before=await probe(); const t=performance.now();
for (let i=0;i<1000;i++) await request(i);
const after=await probe();
const result={requests:1000,before,after,elapsed_ms:performance.now()-t,allocations_per_request:(after.calls-before.calls)/1000,requested_bytes_per_request:(after.bytes-before.bytes)/1000};
writeFileSync(process.argv[2],JSON.stringify(result,null,2)); console.log(result);
