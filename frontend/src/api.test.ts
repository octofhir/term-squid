import { describe, it, expect } from 'vitest'
import { safeReturn } from './api'
describe('administrator return destinations', () => {
  it('retains internal administrator deep links', () => {
    expect(safeReturn('/admin/jobs/123')).toBe('/admin/jobs/123')
    expect(safeReturn('/admin?tab=imports')).toBe('/admin?tab=imports')
  })
  it('rejects external, ambiguous and non-admin destinations', () => {
    for (const value of [null,'https://evil.example','//evil.example','/\\evil.example','/admin\\evil.example','/administer','/login','javascript:alert(1)','/admin\n','/admin/..//evil.example','/admin/%2e%2e//evil.example','/admin/../login']) expect(safeReturn(value)).toBe('/admin')
  })
})
