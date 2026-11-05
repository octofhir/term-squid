// Generated terminology only; an unrelated public lookup must stay correct during cold work.
export const PLACEHOLDER = 'urn:term-squid:mixed:placeholder';
export const CONTEXT_CONCEPTS = 3000;
export function contextFixture(count = CONTEXT_CONCEPTS, depth = 30) {
  if (!Number.isInteger(count) || count < 1 || count > CONTEXT_CONCEPTS || !Number.isInteger(depth) || depth < 1 || depth > 32) throw new Error('Context fixture requires 1..3000 concepts and depth 1..32');
  const concept = [];
  for (let first = 0; first < count; first += depth) {
    let cursor;
    for (let n = first; n < Math.min(first + depth, count); n++) {
      const node = {code: `N${String(n).padStart(5, '0')}`, display: `Generated node ${n}`};
      if (cursor) cursor.concept = [node]; else concept.push(node);
      cursor = node;
    }
  }
  const system = {resourceType:'CodeSystem',url:PLACEHOLDER,version:'1',status:'active',content:'complete',caseSensitive:true,hierarchyMeaning:'is-a',concept};
  if (JSON.stringify(system).length > 256 * 1024) throw new Error('Generated context exceeds public per-resource admission');
  return {resourceType:'Parameters',parameter:[{name:'tx-resource',resource:system},
    {name:'valueSet',resource:{resourceType:'ValueSet',url:PLACEHOLDER+':set',status:'active',compose:{include:[{system:PLACEHOLDER}]}}},
    {name:'count',valueInteger:1},{name:'excludeNested',valueBoolean:true}]};
}
export function contextAccepted(status, body, system, count = CONTEXT_CONCEPTS) {
  const entries = body?.expansion?.contains;
  return status === 200 && body?.resourceType === 'ValueSet' && body.expansion.total === count && Array.isArray(entries) && entries.length === 1 &&
    entries[0]?.system === system && entries[0]?.code === 'N00000' && entries[0]?.display === 'Generated node 0' && (entries[0].version === undefined || entries[0].version === '1');
}
