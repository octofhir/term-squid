import { useEffect, useState } from 'react'

export class ApiError extends Error {
  status: number
  constructor(message: string, status: number) { super(message); this.status = status }
}
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, { credentials: 'same-origin', ...init })
  const body = await response.json().catch(() => null)
  if (!response.ok) {
    if (path.startsWith('/api/admin') && path !== '/api/admin/session' && [401, 403, 503].includes(response.status)) window.dispatchEvent(new Event('session-recheck'))
    throw new ApiError(body?.error?.message ?? body?.issue?.[0]?.diagnostics ?? `Request failed (${response.status})`, response.status)
  }
  return body as T
}
export function useQuery<T>(path: string | null, refresh = 0) {
  const [state, setState] = useState<{ data?: T; error?: Error; loading: boolean }>({ loading: !!path })
  useEffect(() => {
    if (!path) { setState({ loading: false }); return }
    const controller = new AbortController()
    setState({ loading: true })
    request<T>(path, { signal: controller.signal }).then(data => setState({ data, loading: false })).catch(error => {
      if (!controller.signal.aborted) setState({ error, loading: false })
    })
    return () => controller.abort()
  }, [path, refresh])
  return state
}
export interface Resource {
  id: string; resourceType: string; url: string; version?: string; name?: string; title?: string
  status: string; description?: string; publisher?: string; content?: string; nativeProvider?: string
  copyright?: string
}
export interface Page<T> { total: number; items: T[] }
export const kinds = ['CodeSystem', 'ValueSet', 'ConceptMap'] as const
export const labels: Record<string, string> = { CodeSystem: 'Code systems', ValueSet: 'Value sets', ConceptMap: 'Concept maps' }
export const resourceTitle = (r: Resource) => r.title || r.name || r.url
export function safeReturn(value: string | null): string {
  if (!value || /[\\\x00-\x20]/.test(value) || !/^\/admin(?:\/|\?|$)/.test(value)) return '/admin'
  const url = new URL(value, 'https://internal.invalid')
  return url.origin === 'https://internal.invalid' && /^\/admin(?:\/|$)/.test(url.pathname) ? url.pathname + url.search : '/admin'
}
