import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { request } from './api'

interface Session { configured: boolean; administrator: { username: string } | null; csrfToken?: string; expiresAt?: string; capabilities?: string[] }
interface Auth { session: Session | null; loading: boolean; error?: Error; refresh: () => Promise<void>; mutate: <T>(path: string, body?: BodyInit, contentType?: string) => Promise<T> }
const Context = createContext<Auth | null>(null)
export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<Session | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<Error>()
  const sequence = useRef(0)
  const refresh = useCallback(async () => {
    const current = ++sequence.current
    try {
      const next = await request<Session>('/api/admin/session')
      if (current === sequence.current) { setSession(next); setError(undefined) }
    } catch (e) {
      if (current === sequence.current) { setSession(null); setError(e as Error) }
    } finally { if (current === sequence.current) setLoading(false) }
  }, [])
  useEffect(() => {
    void refresh()
    const check = () => { void refresh() }
    window.addEventListener('focus', check)
    window.addEventListener('session-recheck', check)
    const timer = window.setInterval(check, 10_000)
    return () => { clearInterval(timer); window.removeEventListener('focus', check); window.removeEventListener('session-recheck', check) }
  }, [refresh])
  useEffect(() => {
    if (!session?.expiresAt) return
    const timer = window.setTimeout(() => { sequence.current++; setSession(null) }, Math.max(0, new Date(session.expiresAt).getTime() - Date.now()))
    return () => clearTimeout(timer)
  }, [session, refresh])
  const mutate = useCallback(<T,>(path: string, body?: BodyInit, contentType = 'application/json') => request<T>(`/api/admin${path}`, {
    method: 'POST', body, headers: { 'Content-Type': contentType, 'X-CSRF-Token': session?.csrfToken ?? '' },
  }), [session?.csrfToken])
  return <Context.Provider value={{ session, loading, error, refresh, mutate }}>{children}</Context.Provider>
}
export function useAuth() { const context = useContext(Context); if (!context) throw new Error('AuthProvider required'); return context }
export function useCanImport() { const { session, loading } = useAuth(); return !loading && !!session?.administrator && session.capabilities?.includes('import') === true }
