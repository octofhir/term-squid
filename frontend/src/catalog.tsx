import { useState } from 'react'
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { Alert, Button, Card, Modal, Table, TextInput } from '@octofhir/ui-kit'
import { ArrowRight, FileJson, Upload } from 'lucide-react'
import { useCanImport } from './auth'
import { kinds, labels, resourceTitle, useQuery, type Page, type Resource } from './api'
import { Empty, Failure, Heading, JsonView, Loading, Pager, SimpleTable, Status } from './ui'

function ImportAction() { const allowed = useCanImport(), navigate = useNavigate(); return allowed ? <Button onClick={() => navigate('/admin')}><Upload size={15}/> Import resource</Button> : null }
export function Home() {
  const navigate = useNavigate()
  const stats = useQuery<{ code_systems: number; value_sets: number; concept_maps: number }>('/stats')
  const health = useQuery<{ version: string }>('/health')
  const counts = [stats.data?.code_systems, stats.data?.value_sets, stats.data?.concept_maps]
  return <>
    <Heading title="Terminology, connected." description="Explore the terminology available on this server, resolve codes and inspect FHIR resources." actions={<ImportAction/>}/>
    <Failure error={stats.error}/>
    <div className="overview-cards">{kinds.map((kind, i) => <Card key={kind} className="stat-card"><span className="eyebrow">{labels[kind]}</span><strong>{stats.loading ? '…' : counts[i]?.toLocaleString() ?? 'Unavailable'}</strong><Link to={`/catalog/${kind}`}>Browse catalog <ArrowRight size={14}/></Link></Card>)}</div>
    <div className="overview-sections"><section><h2>Work with terminology</h2><p className="muted">Look up a code, validate membership, expand a value set, compare hierarchy or translate a mapping.</p><Button variant="filled" onClick={() => navigate('/operations')}>Open terminology tools <ArrowRight size={15}/></Button></section><section className="server-info"><h2>This server</h2><dl><dt>Product</dt><dd>term-squid {health.data?.version ?? '—'}</dd><dt>Verified adapter</dt><dd>FHIR R4 / 4.0.1</dd><dt>FHIR endpoint</dt><dd><a href="/r4/metadata">/r4</a></dd></dl><p className="muted">Results depend on the content installed here. Unsupported operations return explicit errors.</p></section></div>
  </>
}
export function Catalog() {
  const { kind = '' } = useParams(), [params, setParams] = useSearchParams()
  const offset = Math.max(0, Number(params.get('offset')) || 0)
  const q = params.get('q') ?? '', canonical = params.get('url') ?? ''
  const [draft, setDraft] = useState(q)
  const query = new URLSearchParams({ q, offset: String(offset), ...(canonical ? { url: canonical } : {}) })
  const result = useQuery<Page<Resource>>(kinds.includes(kind as typeof kinds[number]) ? `/api/catalog/${kind}?${query}` : null)
  function update(values: Record<string, string>) { setParams({ q, ...(canonical ? { url: canonical } : {}), ...values }) }
  if (!labels[kind]) return <Failure error="Unknown catalog"/>
  return <>
    <Heading title={labels[kind]} description="Browse canonical resources and their business versions." actions={<ImportAction/>}/>
    <form className="toolbar" onSubmit={e => { e.preventDefault(); update({ q: draft, offset: '0' }) }}><TextInput aria-label="Search catalog" placeholder="Search name, title or canonical…" value={draft} onUpdate={setDraft}/><Button type="submit">Search</Button>{canonical && <Button onClick={() => setParams({})}>Clear canonical filter</Button>}</form>
    {canonical && <p className="canonical">Canonical: {canonical}</p>}
    {result.loading ? <Loading/> : result.error ? <Failure error={result.error}/> : !result.data?.items.length ? <Empty/> : <SimpleTable headings={['Resource', 'Business version', 'Status', 'Publisher']}>
      {result.data.items.map(r => <Table.Tr key={r.id}><Table.Td><Link className="resource-link" to={`/catalog/${kind}/${r.id}`}>{resourceTitle(r)}</Link><div className="canonical">{r.url}</div></Table.Td><Table.Td><code>{r.version ?? 'Unversioned'}</code></Table.Td><Table.Td><Status value={r.status}/></Table.Td><Table.Td>{r.publisher ?? '—'}</Table.Td></Table.Tr>)}
    </SimpleTable>}
    {result.data && <Pager offset={offset} total={result.data.total} onPage={n => update({ offset: String(n) })}/>}
  </>
}
function Versions({ resource, onSelect }: { resource: Resource; onSelect: () => void }) {
  const [offset, setOffset] = useState(0)
  const result = useQuery<Page<Resource>>(`/api/catalog/${resource.resourceType}?${new URLSearchParams({ url: resource.url, offset: String(offset) })}`)
  return <><p className="muted">Select an installed business version of this canonical.</p><Failure error={result.error}/>{result.loading ? <Loading/> : result.data?.items.map(r => <p key={r.id}><Link onClick={onSelect} to={`/catalog/${r.resourceType}/${r.id}`}>{r.version ?? 'Unversioned'}{r.id === resource.id ? ' · selected' : ''}</Link></p>)}{result.data && <Pager offset={offset} total={result.data.total} onPage={setOffset}/>}</>
}
function Concepts({ id }: { id: string }) {
  const [q, setQ] = useState(''), [draft, setDraft] = useState(''), [offset, setOffset] = useState(0)
  const result = useQuery<Page<{ code: string; display?: string; definition?: string }>>(`/api/catalog/CodeSystem/${id}/concepts?${new URLSearchParams({ q, offset: String(offset) })}`)
  return <section className="section"><h2>Concept projections</h2><form className="toolbar" onSubmit={e => { e.preventDefault(); setOffset(0); setQ(draft) }}><TextInput aria-label="Search concepts" placeholder="Search code or display…" value={draft} onUpdate={setDraft}/><Button type="submit">Search</Button></form><Failure error={result.error}/>{result.loading ? <Loading/> : result.data?.items.length ? <SimpleTable headings={['Code', 'Display', 'Definition']}>{result.data.items.map(c => <Table.Tr key={c.code}><Table.Td><code>{c.code}</code></Table.Td><Table.Td>{c.display ?? '—'}</Table.Td><Table.Td>{c.definition ?? '—'}</Table.Td></Table.Tr>)}</SimpleTable> : <Empty>No matching projected concepts. This does not establish completeness of the source terminology.</Empty>}{result.data && <Pager offset={offset} total={result.data.total} onPage={setOffset}/>}</section>
}
function FhirJson({ kind, id }: { kind: string; id: string }) { const result = useQuery<unknown>(`/r4/${kind}/${id}`); return result.loading ? <Loading/> : result.error ? <Failure error={result.error}/> : <JsonView value={result.data}/> }
export function Detail() {
  const { kind = '', id = '' } = useParams()
  const result = useQuery<Resource>(`/api/catalog/${kind}/${id}`)
  const [json, setJson] = useState(false), [versions, setVersions] = useState(false)
  const resource = result.data
  if (result.loading) return <Loading/>
  if (result.error || !resource) return <Failure error={result.error ?? 'Resource unavailable'}/>
  return <>
    <Link to={`/catalog/${kind}`}>← {labels[kind]}</Link>
    <Heading title={resourceTitle(resource)} description={resource.description} actions={<><Button onClick={() => setVersions(true)}>Versions</Button><Button onClick={() => setJson(true)}><FileJson size={15}/> FHIR JSON</Button><ImportAction/></>}/>
    <div className="resource-meta"><Status value={resource.status}/><code>{resource.version ?? 'Unversioned'}</code><span className="canonical">{resource.url}</span></div>
    {resource.content === 'not-present' && <Alert theme="info" message="The resource does not enumerate concepts in its FHIR JSON. Installed projections may still support lookup and validation."/>}
    {resource.nativeProvider && <p className="muted">Native provider: {resource.nativeProvider}. Source-specific operation limits apply.</p>}
    <div className="section"><Link to={`/operations?${new URLSearchParams({ kind, url: resource.url, version: resource.version ?? '', id })}`}>Use this version in terminology tools <ArrowRight size={14}/></Link></div>
    {kind === 'CodeSystem' && <Concepts key={id} id={id}/>}
    {resource.copyright && <p className="muted">{resource.copyright}</p>}
    <Modal open={json} title="FHIR JSON" size="xl" onClose={() => setJson(false)}>{json && <FhirJson kind={kind} id={id}/>}</Modal>
    <Modal open={versions} title="Business versions" onClose={() => setVersions(false)}>{versions && <Versions resource={resource} onSelect={() => setVersions(false)}/>}</Modal>
  </>
}
