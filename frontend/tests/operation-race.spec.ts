import { test, expect } from '@playwright/test'

const system = 'http://example.org/term-squid-benchmark/codes'
test('operation edits cancel previous results and send the declared R4 parameter types', async ({ page }) => {
  let release!: () => void, arrived!: () => void, finished!: () => void
  const gate = new Promise<void>(resolve => { release = resolve })
  const captured = new Promise<void>(resolve => { arrived = resolve })
  const completed = new Promise<void>(resolve => { finished = resolve })
  await page.route('**/r4/CodeSystem/$lookup', async route => {
    try {
      // Delay a real successful response, preserving the server's request and body.
      const response = await route.fetch()
      expect(response.status()).toBe(200)
      arrived()
      await gate
      await route.fulfill({ response })
    } finally { finished() }
  })
  try {
    await page.goto('/operations')
    await page.getByLabel('Canonical URL', {exact: true}).fill(system)
    await page.getByLabel('Code', {exact: true}).fill('C00007')
    await page.getByRole('button', {name: 'Run $lookup', exact: true}).click()
    await captured
    await page.getByRole('combobox', {name: 'Operation', exact: true}).click()
    await page.getByRole('option', {name: '$validate-code', exact: true}).click()
    await expect(page.getByRole('region', {name: 'Operation result'})).toHaveCount(0)
    const run = page.getByRole('button', {name: 'Run $validate-code', exact: true})
    await expect(run).toBeEnabled()
    const submitted = page.waitForRequest(request => request.method() === 'POST' && new URL(request.url()).pathname === '/r4/CodeSystem/$validate-code')
    await run.click()
    const input = (await submitted).postDataJSON()
    expect(input.parameter.find((parameter: {name: string}) => parameter.name === 'url')).toEqual({name: 'url', valueUri: system})
    expect(input.parameter.find((parameter: {name: string}) => parameter.name === 'code')).toEqual({name: 'code', valueCode: 'C00007'})
    expect(input.parameter.some((parameter: {name: string}) => parameter.name === 'system')).toBe(false)
    const result = page.getByRole('region', {name: 'Operation result'})
    await expect(result).toContainText('"valueBoolean": true')
    release(); await completed
    await page.waitForLoadState('networkidle')
    await expect(result).toContainText('"valueBoolean": true')
    await expect(result).not.toContainText('TermSquidBenchmarkCodes')
    await page.getByLabel('Code', {exact: true}).fill('C00008')
    await expect(result).toHaveCount(0)
  } finally { release() }
})
