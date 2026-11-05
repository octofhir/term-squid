import { test, expect } from '@playwright/test'
for (const width of [390, 768, 1024, 1440]) for (const colorScheme of ['light', 'dark'] as const) {
  test(`bounded pages ${width} ${colorScheme}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 }); await page.emulateMedia({ colorScheme })
    const errors: string[] = []; page.on('pageerror', e => errors.push(e.message))
    const catalog = await (await page.request.get('/api/catalog/CodeSystem')).json()
    const paths = ['/', '/catalog/CodeSystem', '/catalog/ValueSet', '/catalog/ConceptMap', '/operations', '/login', `/catalog/CodeSystem/${catalog.items[0].id}`]
    for (const path of paths) {
      await page.goto(path); await expect(page.locator('h1')).toBeVisible()
      await expect(page.getByText('Loading…', { exact: true })).toHaveCount(0)
      await page.waitForLoadState('networkidle')
      const overflow = await page.evaluate(() => [...document.querySelectorAll('main, #root, .page, body, header')].map(el => ({ tag: el.tagName, class: el.className, client: el.clientWidth, scroll: el.scrollWidth })).filter(x => x.scroll>x.client+1))
      expect(overflow, path).toEqual([])
      if (width === 390 && path.startsWith('/catalog/')) {
        const region = page.getByRole('region', {name: path.split('/').length === 3 ? 'Catalog resources' : 'Concept projections', exact: true})
        await expect(region).toBeVisible()
        expect(await region.evaluate(el => el.scrollWidth > el.clientWidth)).toBe(true)
        if (path.split('/').length === 3) {
          expect(await region.locator('tbody td').first().evaluate(el => el.getBoundingClientRect().width)).toBeGreaterThanOrEqual(200)
        }
        await region.focus(); await page.keyboard.press('ArrowRight')
        await expect.poll(() => region.evaluate(el => el.scrollLeft)).toBeGreaterThan(0)
        await region.evaluate(el => { el.scrollLeft = 0 })
      }
      await page.screenshot({ path: `../.benchmarks/ui-admin/screenshots/${width}-${colorScheme}-${path.split('/').filter(Boolean).join('-') || 'home'}.png`, fullPage: true })
    }
    expect(errors).toEqual([])
  })
}

test('mobile navigation opens, follows a link and closes', async ({ page }) => {
  await page.setViewportSize({width:390,height:844});await page.goto('/')
  const toggle=page.getByRole('button',{name:'Toggle navigation',exact:true})
  await toggle.click();await expect(toggle).toHaveAttribute('aria-expanded','true')
  await page.getByRole('button',{name:'Code systems',exact:true}).click()
  await expect(page.getByRole('heading',{name:'Code systems',exact:true})).toBeVisible()
  await expect(toggle).toHaveAttribute('aria-expanded','false')
})
