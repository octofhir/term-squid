import { useEffect, useState, type ReactNode } from 'react'
import { Link, Navigate, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { Alert, Button, Card, Modal, PasswordInput, Select, Table, TextInput } from '@octofhir/ui-kit'
import { Upload } from 'lucide-react'
import { useAuth } from './auth'
import { request, safeReturn, useQuery } from './api'
import { Empty, Failure, Heading, JsonView, Loading, SimpleTable, Status } from './ui'

export function RequireAdmin({ children }: { children: ReactNode }) {
  const { session, loading } = useAuth(), location = useLocation()
  if (loading) return <Loading/>
  if (!session?.administrator) return <Navigate to={`/login?returnTo=${encodeURIComponent(location.pathname + location.search)}`} replace/>
  return children
}
export function Login() {
  const auth = useAuth(), navigate = useNavigate(), [params] = useSearchParams()
  const [username, setUsername] = useState(''), [password, setPassword] = useState(''), [busy, setBusy] = useState(false), [error, setError] = useState<Error>()
  if (auth.session?.administrator) return <Navigate to={safeReturn(params.get('returnTo'))} replace/>
  const unavailable = !auth.loading && (!auth.session?.configured || !!auth.error)
  return <div className="login-scene"><div className="login-brand"><img src="/branding/icon-192.png" alt=""/><h1>term-squid</h1><p className="muted">Sign in to administer your terminology server</p></div><Card view="raised" className="login-panel"><form onSubmit={async e => {
    e.preventDefault(); setBusy(true); setError(undefined)
    try { await request('/api/admin/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username, password }) }); setPassword(''); await auth.refresh(); navigate(safeReturn(params.get('returnTo')), { replace: true }) }
    catch (e) { setError(e as Error) } finally { setBusy(false) }
  }}><h2>Welcome back</h2><p className="muted">Use your standalone administrator credentials.</p><Failure error={error}/>{unavailable && <Alert theme="warning" message={auth.error ? 'Administration is temporarily unavailable. Public browsing remains available.' : 'Administration is not configured. Ask the server operator to configure a bootstrap administrator.'}/>}
    <TextInput aria-label="Username" label="Username" value={username} onUpdate={setUsername} required autoFocus disabled={busy || unavailable} {...{ autoComplete: 'username', 'aria-required': true }}/><PasswordInput aria-label="Password" {...{ 'aria-required': true }} label="Password" value={password} onUpdate={setPassword} autoComplete="current-password" required disabled={busy || unavailable}/><Button type="submit" variant="filled" width="max" size="lg" disabled={!username || !password || unavailable || auth.loading} loading={busy}>Sign in</Button><Link to="/">Continue browsing terminology</Link>
  </form></Card></div>
}
interface Inspection { resourceType: string; url: string; version?: string; records: number; recordKind?: string; resources?: Inspection[]; dependencies?: Record<string, string> }
interface ResourceResult { id: string; unchanged: boolean; resourceType: string; url: string; version?: string }
interface Job { id: string; state: string; bytes: number; sha256?: string; format: string; inspection?: Inspection; result?: { id?: string; unchanged: boolean | number; imported?: number; resources?: ResourceResult[] }; error?: string; cancel_requested: boolean; created_at: string; updated_at: string }
const terminal = (state: string) => ['succeeded','failed','cancelled','interrupted'].includes(state)
export function Admin() {
  const auth = useAuth(), navigate = useNavigate()
  const [format, setFormat] = useState('fhir-json')
  const byteLimit = (format === 'fhir-package' ? 32 : 16) * 1024 * 1024
  const [refresh, setRefresh] = useState(0), [file, setFile] = useState<File>(), [busy, setBusy] = useState(false), [error, setError] = useState<Error>()
  const jobs = useQuery<{ jobs: Job[] }>('/api/admin/jobs', refresh)
  useEffect(() => { const timer = window.setInterval(() => setRefresh(v => v+1), 5000); return () => clearInterval(timer) }, [])
  return <><Heading title="Administration" description={`Signed in as ${auth.session?.administrator?.username}. Import and activate terminology resources.`}/>
    <Card className="upload-panel"><h2>Import terminology</h2><Select label="Upload format" data={[{value:'fhir-json',label:'Single FHIR JSON resource'},{value:'fhir-package',label:'Local FHIR package (.tgz)'}]} value={format} onUpdate={value => { if (value) { setFormat(value); setFile(undefined); setError(undefined) } }} disabled={busy}/><p className="muted">{format === 'fhir-package' ? 'One local R4 npm-format FHIR package declaring FHIR 4.0.1. Up to 32 MiB compressed, 128 MiB decompressed, 256 terminology resources and 50,000 concepts or definition entries in total. Package dependencies are not downloaded.' : 'One UTF-8 R4 CodeSystem, ValueSet or ConceptMap JSON file, up to 16 MiB and 50,000 concepts or definition entries.'} Inspect it before starting atomic activation.</p><form className="toolbar" onSubmit={async e => {
      e.preventDefault(); if (!file) return; setBusy(true); setError(undefined)
      try { const job = await auth.mutate<Job>('/jobs',file,format === 'fhir-package' ? 'application/gzip' : 'application/fhir+json'); navigate(`/admin/jobs/${job.id}`) }
      catch (e) { setError(e as Error); setRefresh(v => v+1) } finally { setBusy(false) }
    }}><label className="file-picker">{format === 'fhir-package' ? 'FHIR package file' : 'FHIR JSON file'}<input key={format} type="file" accept={format === 'fhir-package' ? '.tgz,.tar.gz,application/gzip' : '.json,application/fhir+json,application/json'} disabled={busy} onChange={e => { setFile(e.target.files?.[0]); setError(undefined) }}/></label><Button type="submit" variant="filled" disabled={!file || file.size > byteLimit} loading={busy}><Upload size={15}/> Upload and inspect</Button></form>{file && file.size > byteLimit && <Failure error={`File exceeds ${byteLimit / 1024 / 1024} MiB`}/>}<Failure error={error}/>{busy && <p role="status">Uploading and inspecting… Activation only starts after confirmation.</p>}</Card>
    <Alert theme="neutral" message="Bundles and arbitrary archives are unsupported. Packages supply only their local CodeSystem, ValueSet and ConceptMap resources; other resource types are ignored. Native SNOMED RF2 and LOINC require the CLI's checksummed manifest and source files. RxNorm and LOINC accessory imports are not implemented."/>
    <section className="section"><h2>Import history</h2><p className="muted">Latest 100 jobs · history retained for 7 days · unstarted inspections expire after 24 hours.</p><Failure error={jobs.error}/>{jobs.loading ? <Loading/> : jobs.data?.jobs.length ? <SimpleTable minWidth={800} label="Import history" headings={['Resource / job', 'Stage', 'Uploaded', 'Created']}>{jobs.data.jobs.map(job => <Table.Tr key={job.id}><Table.Td><Link to={`/admin/jobs/${job.id}`}>{job.inspection?.url ?? job.id}</Link><div className="muted">{job.inspection?.version}</div></Table.Td><Table.Td><Status value={job.state}/></Table.Td><Table.Td>{job.bytes.toLocaleString()} bytes</Table.Td><Table.Td>{new Date(job.created_at).toLocaleString()}</Table.Td></Table.Tr>)}</SimpleTable> : <Empty>No imports yet. Upload a supported resource to begin.</Empty>}</section>
  </>
}
export function JobDetail() {
  const { id = '' } = useParams(), auth = useAuth()
  const [refresh, setRefresh] = useState(0), [error, setError] = useState<Error>(), [busy, setBusy] = useState(false), [confirm, setConfirm] = useState(false)
  const result = useQuery<Job>(`/api/admin/jobs/${encodeURIComponent(id)}`,refresh), job = result.data
  useEffect(() => { if (job && (terminal(job.state) || job.state === 'ready')) return; const timer = window.setTimeout(() => setRefresh(v => v+1),1500); return () => clearTimeout(timer) }, [job,refresh])
  async function action(name: string) { setBusy(true); setError(undefined); try { await auth.mutate(`/jobs/${id}/${name}`); setConfirm(false); setRefresh(v=>v+1) } catch (e) { setError(e as Error) } finally { setBusy(false) } }
  if (!job) return result.loading ? <Loading/> : <Failure error={result.error}/>
  return <><Link to="/admin">← Administration</Link><Heading title="Import job" description={job.id}/><Failure error={error ?? result.error}/>
    <div className="resource-meta"><Status value={job.state}/><span>{job.bytes.toLocaleString()} bytes</span>{job.inspection && <span>{job.inspection.records.toLocaleString()} {job.inspection.recordKind ?? (job.inspection.resourceType === 'CodeSystem' ? 'concepts' : 'resource')}</span>}</div>
    {job.inspection && <Card className="job-inspection"><h2>{job.inspection.resourceType}</h2><p className="canonical">{job.inspection.url}</p><p>{job.inspection.resources ? 'Package version' : 'Business version'}: {job.inspection.version ?? 'Unversioned'}</p>{job.inspection.resources && <><p>{job.inspection.resources.length} terminology resources · all activate together</p><SimpleTable minWidth={600} label="Package resource inventory" headings={['Resource', 'Business version', 'Records']}>{job.inspection.resources.map(meta => <Table.Tr key={`${meta.resourceType}|${meta.url}|${meta.version}`}><Table.Td>{meta.resourceType}<div className="canonical">{meta.url}</div></Table.Td><Table.Td>{meta.version ?? 'Unversioned'}</Table.Td><Table.Td>{meta.records.toLocaleString()} {meta.recordKind}</Table.Td></Table.Tr>)}</SimpleTable>{Object.keys(job.inspection.dependencies ?? {}).length > 0 && <Alert theme="warning" message="Dependencies are declared by this package but are not installed automatically. Install required terminology separately before using dependent definitions."/>}</>}</Card>}{job.sha256 && <p className="checksum">SHA-256 <code>{job.sha256}</code></p>}{job.error && <Failure error={job.error}/>}
    {job.state === 'ready' && <Alert theme="info" message="Inspection passed. Activation verifies the source checksum and commits content and projections atomically. Existing business versions are immutable."/>}
    {!terminal(job.state) && job.state !== 'ready' && <p role="status">{job.cancel_requested ? 'Cancellation requested. Waiting for rollback…' : `Current stage: ${job.state}. Progress totals are unavailable during this stage.`}</p>}
    <div className="actions section">{job.state === 'ready' && <Button variant="filled" loading={busy} onClick={() => { void action('start') }}>Start import</Button>}{['ready','queued','validating','publishing'].includes(job.state) && <Button disabled={busy || job.cancel_requested} onClick={() => setConfirm(true)}>Cancel import</Button>}</div>
    {job.state === 'succeeded' && job.result && <Alert theme="success" message={job.result.resources ? `${job.result.imported} resources activated; ${job.result.unchanged} identical resources already installed.` : job.result.unchanged ? 'Identical content is already installed. No publication change was needed.' : 'Content activated successfully.'} actions={job.result.id ? <Link to={`/catalog/${job.inspection?.resourceType}/${job.result.id}`}>Browse activated resource →</Link> : undefined}/>}
    {job.state === 'succeeded' && job.result?.resources && <SimpleTable minWidth={600} label="Activated resources" headings={['Activated resource', 'Business version', 'Result']}>{job.result.resources.map(resource => <Table.Tr key={resource.id}><Table.Td><Link to={`/catalog/${resource.resourceType}/${resource.id}`}>{resource.url}</Link><div className="muted">{resource.resourceType}</div></Table.Td><Table.Td>{resource.version ?? 'Unversioned'}</Table.Td><Table.Td>{resource.unchanged ? 'Already installed' : 'Activated'}</Table.Td></Table.Tr>)}</SimpleTable>}
    {job.result && <JsonView value={job.result}/>}
    <Modal open={confirm} title="Cancel import?" onClose={() => setConfirm(false)} footer={<><Button onClick={() => setConfirm(false)}>Keep import</Button><Button variant="filled" loading={busy} onClick={() => { void action('cancel') }}>Confirm cancellation</Button></>}><p>Cancellation rolls back uncommitted content. A completed activation remains published.</p></Modal>
  </>
}
