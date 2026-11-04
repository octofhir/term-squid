import { useState } from 'react'
import { BrowserRouter, Link, Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import { ActionIcon, Button, LocationBar, Shell, Sidebar, UIProvider, useColorScheme } from '@octofhir/ui-kit'
import { ArrowLeftRight, BookOpen, Database, Home as HomeIcon, LogIn, Menu, Moon, Search, Shield, Sun } from 'lucide-react'
import { AuthProvider, useAuth, useCanImport } from './auth'
import { useQuery, kinds, labels } from './api'
import { Failure } from './ui'
import { Home, Catalog, Detail } from './catalog'
import { Operations } from './operations'
import { Admin, JobDetail, Login, RequireAdmin } from './admin'
import './styles.css'

function Frame() {
  const navigate = useNavigate(), location = useLocation()
  const { colorScheme, toggleColorScheme } = useColorScheme()
  const { session, mutate, refresh } = useAuth()
  const canImport = useCanImport()
  const health = useQuery<{ status: string }>('/health')
  const [menu, setMenu] = useState(false)
  const [error, setError] = useState<Error>()
  const go = (path: string) => { navigate(path); setMenu(false) }
  const icons = [Database, BookOpen, ArrowLeftRight]
  const items = [{ id: '/', label: 'Overview', icon: HomeIcon }, ...kinds.map((kind, i) => ({ id: `/catalog/${kind}`, label: labels[kind], icon: icons[i] })), { id: '/operations', label: 'Terminology tools', icon: Search }]
  const groups = [{ id: 'public', label: 'Terminology', items: items.map(i => ({ ...i, active: i.id === '/' ? location.pathname === '/' : location.pathname.startsWith(i.id), onClick: () => go(i.id) })) }, ...(canImport ? [{ id: 'admin', label: 'Manage', items: [{ id: '/admin', label: 'Administration', icon: Shield, active: location.pathname.startsWith('/admin'), onClick: () => go('/admin') }] }] : [])]
  async function logout() { try { await mutate('/logout'); await refresh(); go('/') } catch (e) { setError(e as Error) } }
  const segments = [{ id: 'root', label: 'term-squid', onClick: () => go('/') }, ...location.pathname.split('/').filter(Boolean).map((part, i) => ({ id: String(i), label: labels[part] ?? part }))]
  return <><a href="#content" className="skip-link">Skip to content</a><Shell className={menu ? 'app mobile-menu' : 'app'} sidebar={<Sidebar className="product-sidebar" tone="ink" brand={{ title: 'term-squid', iconSrc: '/branding/icon-192.png', onClick: () => go('/') }} groups={groups} status={{ label: health.error ? 'Server unavailable' : health.data?.status ?? 'Connecting…', theme: health.data?.status === 'ok' ? 'success' : 'warning' }} account={session?.administrator ? { name: session.administrator.username, secondary: 'Administrator', onSignOut: () => { void logout() } } : null} colorScheme={colorScheme} onToggleColorScheme={toggleColorScheme} />}
    header={<LocationBar segments={segments} meta="FHIR R4" actions={<><ActionIcon className="mobile-toggle" aria-label="Toggle navigation" aria-expanded={menu} onClick={() => setMenu(!menu)}><Menu size={18}/></ActionIcon><ActionIcon variant="subtle" aria-label={colorScheme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'} onClick={toggleColorScheme}>{colorScheme === 'dark' ? <Sun size={16}/> : <Moon size={16}/>}</ActionIcon>{!session?.administrator && <Button variant="subtle" onClick={() => go('/login')}><LogIn size={14}/> Sign in</Button>}</>} />}>
    <div className="page" id="content" tabIndex={-1}><Failure error={error}/><Routes>
      <Route path="/" element={<Home/>}/><Route path="/catalog/:kind" element={<Catalog/>}/><Route path="/catalog/:kind/:id" element={<Detail/>}/>
      <Route path="/operations" element={<Operations/>}/><Route path="/login" element={<Login/>}/>
      <Route path="/admin" element={<RequireAdmin><Admin/></RequireAdmin>}/><Route path="/admin/jobs/:id" element={<RequireAdmin><JobDetail/></RequireAdmin>}/>
      <Route path="/codesystems" element={<Navigate to="/catalog/CodeSystem" replace/>}/><Route path="/valuesets" element={<Navigate to="/catalog/ValueSet" replace/>}/><Route path="/conceptmaps" element={<Navigate to="/catalog/ConceptMap" replace/>}/>
      <Route path="*" element={<><h1>Page not found</h1><Link to="/">Return to overview</Link></>}/>
    </Routes></div>
  </Shell></>
}
export default function App() { return <UIProvider><BrowserRouter><AuthProvider><Frame/></AuthProvider></BrowserRouter></UIProvider> }
