import { useEffect, useRef, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Alert, Button, Select, TextInput } from '@octofhir/ui-kit'
import { request, kinds } from './api'
import { Failure, Heading, JsonView } from './ui'

const fieldLabels: Record<string,string> = { codeA: 'Code A', codeB: 'Code B', code: 'Code', system: 'Code system URI', filter: 'Filter (optional)' }
const available: Record<string, string[]> = { CodeSystem: ['lookup', 'validate-code', 'subsumes'], ValueSet: ['validate-code', 'expand'], ConceptMap: ['translate'] }
export function Operations() {
  const [params] = useSearchParams()
  const initialKind = kinds.includes(params.get('kind') as typeof kinds[number]) ? params.get('kind')! : 'CodeSystem'
  const [kind, setKind] = useState(initialKind), [op, setOp] = useState(available[initialKind][0])
  const [url, setUrl] = useState(params.get('url') ?? ''), [version, setVersion] = useState(params.get('version') ?? '')
  const [id, setId] = useState(params.get('id') ?? ''), [method, setMethod] = useState('POST')
  const [fields, setFields] = useState<Record<string, string>>({}), [result, setResult] = useState<unknown>(), [error, setError] = useState<Error>(), [busy, setBusy] = useState(false)
  const [pageOffset, setPageOffset] = useState(0)
  const sequence = useRef(0), active = useRef<AbortController | null>(null)
  useEffect(() => () => { sequence.current++; active.current?.abort() }, [])
  function reset() { sequence.current++; active.current?.abort(); active.current = null; setBusy(false); setResult(undefined); setError(undefined); setPageOffset(0) }

  const fieldNames = op === 'subsumes' ? ['codeA','codeB'] : op === 'expand' ? ['filter'] : kind === 'ValueSet' || kind === 'ConceptMap' ? ['system','code'] : ['code']
  async function run(offset = 0) {
    const current = ++sequence.current, controller = new AbortController()
    active.current?.abort(); active.current = controller
    setBusy(true); setError(undefined); setResult(undefined)
    try {
      const values: Record<string, string> = Object.fromEntries(Object.entries(fields).filter(([k, v]) => fieldNames.includes(k) && v !== ''))
      if (!id) {
        values[kind === 'CodeSystem' && op !== 'validate-code' ? 'system' : 'url'] = url
        if (version) values[kind === 'ValueSet' ? 'valueSetVersion' : kind === 'ConceptMap' ? 'conceptMapVersion' : 'version'] = version
      }
      if (op === 'expand') { values.count = '25'; values.offset = String(offset) }
      const path = `/r4/${kind}/${id ? `${encodeURIComponent(id)}/` : ''}$${op}`
      const body = { resourceType: 'Parameters', parameter: Object.entries(values).map(([name, value]) => ({ name, ...(['offset','count'].includes(name) ? { valueInteger: Number(value) } : ['system','url'].includes(name) ? { valueUri: value } : ['code','codeA','codeB'].includes(name) ? { valueCode: value } : { valueString: value }) })) }
      const data = await request<unknown>(method === 'GET' ? `${path}?${new URLSearchParams(values)}` : path, method === 'GET' ? { signal: controller.signal } : { signal: controller.signal, method: 'POST', headers: { 'Content-Type': 'application/fhir+json' }, body: JSON.stringify(body) })
      if (current === sequence.current) { setResult(data); setPageOffset(offset) }
    } catch (e) { if (current === sequence.current && !controller.signal.aborted) setError(e as Error) }
    finally { if (current === sequence.current) { setBusy(false); active.current = null } }
  }
  const total = (result as { expansion?: { total?: number } } | undefined)?.expansion?.total
  return <>
    <Heading title="Terminology tools" description="Run supported read operations against the verified FHIR R4 adapter."/>
    <Alert theme="neutral" message="Installed content and provider capabilities determine the result. Unsupported expressions are reported explicitly."/>
    <form className="operation-form" onSubmit={e => { e.preventDefault(); void run() }}>
      <div className="form-grid"><Select label="Resource type" data={[...kinds]} value={kind} onUpdate={v => { if (v) { reset(); setKind(v); setOp(available[v][0]); setId(''); setUrl(''); setVersion(''); setResult(undefined) } }}/><Select label="Operation" data={available[kind].map(v => ({ value: v, label: `$${v}` }))} value={op} onUpdate={v => { if (v) { reset(); setOp(v); setResult(undefined) } }}/><Select label="HTTP method" data={['POST','GET']} value={method} onUpdate={v => { if (v) { reset(); setMethod(v) } }}/></div>
      {id ? <Alert theme="info" message={`Using the selected resource instance (${version || 'unversioned'}).`} actions={<Button onClick={() => { reset(); setId('') }}>Use canonical instead</Button>}/> : <div className="form-grid"><TextInput aria-label="Canonical URL" {...{ 'aria-required': true }} label="Canonical URL" required value={url} onUpdate={value => { reset(); setUrl(value) }} placeholder="https://…"/><TextInput label="Business version (optional)" value={version} onUpdate={value => { reset(); setVersion(value) }}/></div>}
      <div className="form-grid">{fieldNames.map(name => <TextInput key={name} aria-label={fieldLabels[name]} label={fieldLabels[name]} {...{ 'aria-required': name !== 'filter' }} required={name !== 'filter'} value={fields[name] ?? ''} onUpdate={value => { reset(); setFields(prev => ({ ...prev, [name]: value })) }}/>)}</div>
      <Button variant="filled" type="submit" loading={busy}>Run ${op}</Button>
    </form>
    <Failure error={error}/>{result !== undefined && <section className="section" aria-label="Operation result"><h2>Result</h2>{op === 'expand' && typeof total === 'number' && <div className="pager"><span>{total.toLocaleString()} total · offset {pageOffset}</span><div className="actions"><Button disabled={busy || !pageOffset} onClick={() => { void run(Math.max(0,pageOffset-25)) }}>Previous expansion page</Button><Button disabled={busy || pageOffset+25>=total} onClick={() => { void run(pageOffset+25) }}>Next expansion page</Button></div></div>}<JsonView value={result}/></section>}
  </>
}
