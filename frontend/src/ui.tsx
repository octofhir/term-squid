import { Alert, Button, Badge, Table, Text } from '@octofhir/ui-kit'
import { ArrowLeft, ArrowRight } from 'lucide-react'
import type { ReactNode } from 'react'

export function Failure({ error }: { error?: Error | string }) { return error ? <Alert role="alert" theme="danger" message={typeof error === 'string' ? error : error.message} /> : null }
export function Loading() { return <p role="status" className="empty">Loading…</p> }
export function Empty({ children = 'No results. Try another search.' }: { children?: ReactNode }) { return <div className="empty">{children}</div> }
export function Status({ value }: { value: string }) { return <Badge theme={['active','succeeded'].includes(value) ? 'success' : ['failed','interrupted'].includes(value) ? 'danger' : ['queued','publishing','validating','inspecting'].includes(value) ? 'info' : 'normal'}>{value}</Badge> }
export function Heading({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) { return <div className="page-heading"><div><h1>{title}</h1>{description && <p className="muted">{description}</p>}</div><div className="actions">{actions}</div></div> }
export function Pager({ offset, total, onPage, size = 25 }: { offset: number; total: number; onPage: (n: number) => void; size?: number }) {
  return <nav className="pager" aria-label="Pagination"><Text color="secondary">{total ? `${offset + 1}–${Math.min(offset + size, total)} of ${total.toLocaleString()}` : '0 results'}</Text><div className="actions"><Button aria-label="Previous page" disabled={!offset} onClick={() => onPage(Math.max(0, offset - size))}><ArrowLeft size={14} /> Previous</Button><Button aria-label="Next page" disabled={offset + size >= total} onClick={() => onPage(offset + size)}>Next <ArrowRight size={14} /></Button></div></nav>
}
export function JsonView({ value }: { value: unknown }) { return <pre className="json" tabIndex={0}>{JSON.stringify(value, null, 2)}</pre> }
export function SimpleTable({ headings, children, minWidth, label }: { headings: string[]; children: ReactNode; minWidth?: number; label?: string }) { return <Table.ScrollContainer minWidth={minWidth} tabIndex={minWidth ? 0 : undefined} role={minWidth ? 'region' : undefined} aria-label={label}><Table highlightOnHover verticalSpacing="md"><Table.Thead><Table.Tr>{headings.map(h => <Table.Th key={h}>{h}</Table.Th>)}</Table.Tr></Table.Thead><Table.Tbody>{children}</Table.Tbody></Table></Table.ScrollContainer> }
