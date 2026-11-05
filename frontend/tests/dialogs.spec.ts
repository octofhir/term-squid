import { test, expect } from '@playwright/test'
for (const width of [390,1440]) for (const colorScheme of ['light','dark'] as const) {
  test(`resource dialogs fit and keep keyboard focus ${width} ${colorScheme}`, async ({page})=>{
    await page.setViewportSize({width,height:900});await page.emulateMedia({colorScheme})
    const catalog=await (await page.request.get('/api/catalog/CodeSystem')).json()
    const resource=catalog.items.find((r:{name:string})=>r.name==='TermSquidBenchmarkCodes')
    await page.goto(`/catalog/CodeSystem/${resource.id}`)
    for (const name of ['Versions','FHIR JSON']) {
      await page.getByRole('button',{name,exact:true}).click()
      const dialog=page.getByRole('dialog');await expect(dialog).toBeVisible()
      if(name==='FHIR JSON') await expect(dialog).toContainText('CodeSystem')
      else await expect(dialog).toContainText(`${resource.version} · selected`)
      // Capture the actual kit's final surface, not its 150 ms entrance transition.
      await expect(dialog).toHaveCSS('opacity','1')
      await page.keyboard.press('Tab')
      expect(await dialog.evaluate(el=>el.contains(document.activeElement))).toBe(true)
      const box=(await dialog.boundingBox())!
      expect(box.x).toBeGreaterThanOrEqual(0);expect(box.x+box.width).toBeLessThanOrEqual(width+1)
      await page.screenshot({path:`../.benchmarks/ui-admin/screenshots/dialog-${width}-${colorScheme}-${name.replace(' ','-')}.png`,fullPage:true,animations:'disabled'})
      await page.keyboard.press('Escape');await expect(dialog).toHaveCount(0)
    }
  })
}
