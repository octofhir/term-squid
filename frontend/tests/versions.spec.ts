import { test, expect } from '@playwright/test'
import { mkdir, readFile, writeFile } from 'node:fs/promises'

test('anonymous version selection opens the activated immutable edition and its operation context', async ({ page }) => {
  test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE, 'Requires the local test administrator')
  const origin = process.env.UI_BASE_URL ?? 'http://127.0.0.1:8082'
  const password = (await readFile(process.env.ADMIN_TEST_PASSWORD_FILE!, 'utf8')).trim()
  expect((await page.request.post('/api/admin/login', { headers: { Origin: origin }, data: { username: 'admin', password } })).status()).toBe(200)
  const session = await (await page.request.get('/api/admin/session')).json()
  const headers = { Origin: origin, 'X-CSRF-Token': session.csrfToken, 'Content-Type': 'application/fhir+json' }
  const canonical = `urn:term-squid:browser:versions:${Date.now()}`
  const editions: Record<string, string> = {}
  const directory = '../.benchmarks/ui-admin/uploads'
  await mkdir(directory, { recursive: true })
  for (const [version, display] of [['1', 'First edition'], ['2', 'Second edition']]) {
    const raw = JSON.stringify({ resourceType: 'CodeSystem', url: canonical, version, name: 'BrowserVersionFixture', status: 'active', content: 'complete', concept: [{ code: 'edition-code', display }] })
    await writeFile(`${directory}/versions-${canonical.split(':').at(-1)}-${version}.json`, raw)
    const upload = await page.request.post('/api/admin/jobs', { headers, data: raw })
    expect(upload.status()).toBe(200)
    const job = await upload.json()
    expect((await page.request.post(`/api/admin/jobs/${job.id}/start`, { headers, data: {} })).status()).toBe(200)
    await expect.poll(async () => (await (await page.request.get(`/api/admin/jobs/${job.id}`)).json()).state).toBe('succeeded')
    const activated = await (await page.request.get(`/api/admin/jobs/${job.id}`)).json()
    editions[version] = activated.result.id
  }
  expect((await page.request.post('/api/admin/logout', { headers, data: {} })).status()).toBe(200)
  expect((await page.request.get('/api/admin/jobs')).status()).toBe(401)

  const catalog = await (await page.request.get(`/api/catalog/CodeSystem?${new URLSearchParams({ url: canonical })}`)).json()
  expect(catalog.total).toBe(2)
  expect(catalog.items.map((resource: { version: string }) => resource.version).sort()).toEqual(['1', '2'])
  await page.goto(`/catalog/CodeSystem/${editions['2']}`)
  await expect(page.locator('main')).toContainText('Second edition')
  await expect(page.getByRole('button', { name: 'Import resource', exact: true })).toHaveCount(0)
  await page.getByRole('button', { name: 'Versions', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Business versions', exact: true })
  await expect(dialog.getByRole('link', { name: '2 · selected', exact: true })).toBeVisible()
  await dialog.getByRole('link', { name: '1', exact: true }).click()
  await expect(page).toHaveURL(new RegExp(`/catalog/CodeSystem/${editions['1']}$`))
  await expect(page.getByRole('dialog')).toHaveCount(0)
  await expect(page.locator('main')).toContainText('First edition')
  await expect(page.locator('main')).not.toContainText('Second edition')
  await page.getByRole('link', { name: /Use this version in terminology tools/ }).click()
  expect(new URL(page.url()).searchParams.get('id')).toBe(editions['1'])
  expect(new URL(page.url()).searchParams.get('version')).toBe('1')
  await page.getByLabel('Code', { exact: true }).fill('edition-code')
  await page.getByRole('button', { name: 'Run $lookup', exact: true }).click()
  const result = page.getByRole('region', { name: 'Operation result' })
  await expect(result).toContainText('First edition')
  await expect(result).toContainText('"valueString": "1"')
  await expect(result).not.toContainText('Second edition')
})
