import { test, expect } from '@playwright/test'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { gzipSync } from 'node:zlib'

// Generate local tar fixtures without introducing a browser-side archive parser.
function packageArchive(name: string, resources: object[]) {
  const files: [string,object][] = [['package/package.json',{name,version:'1.0.0',fhirVersions:['4.0.1'],dependencies:{'org.example.prerequisite':'1.0.0'}}],...resources.map((resource,index):[string,object]=>[`package/resource-${index}.json`,resource])]
  const blocks:Buffer[]=[]
  for(const [path,resource] of files) {
    const data=Buffer.from(JSON.stringify(resource)), header=Buffer.alloc(512)
    header.write(path);header.write('0000600\0',100);header.write('0000000\0',108);header.write('0000000\0',116)
    header.write(data.length.toString(8).padStart(11,'0')+'\0',124);header.write('00000000000\0',136)
    header.fill(32,148,156);header.write('0',156);header.write('ustar\0',257);header.write('00',263)
    header.write([...header].reduce((sum,byte)=>sum+byte,0).toString(8).padStart(6,'0')+'\0 ',148)
    blocks.push(header,data,Buffer.alloc((512-data.length%512)%512))
  }
  return gzipSync(Buffer.concat([...blocks,Buffer.alloc(1024)]))
}
test('package import is atomic and usable on desktop/mobile in both themes',async({page})=>{
    test.setTimeout(60_000)
    test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE,'Requires the local test administrator')
    await page.goto('/admin');await page.getByLabel('Username',{exact:true}).fill('admin')
    await page.getByLabel('Password',{exact:true}).fill((await readFile(process.env.ADMIN_TEST_PASSWORD_FILE!,'utf8')).trim())
    await page.locator('main form').getByRole('button',{name:'Sign in',exact:true}).click()
    await expect(page.getByRole('heading',{name:'Administration',exact:true})).toBeVisible()
    for(const width of [390,1440]) for(const colorScheme of ['light','dark'] as const) {
    await page.setViewportSize({width,height:900});await page.emulateMedia({colorScheme})
    await page.goto('/admin')
    await page.getByRole('combobox',{name:'Upload format',exact:true}).click()
    await page.getByRole('option',{name:'Local FHIR package (.tgz)',exact:true}).click()
    const canonical=`urn:term-squid:package-browser:${width}:${colorScheme}:${Date.now()}`, set=canonical+':set'
    const raw=packageArchive('org.example.browser-package',[
      {resourceType:'CodeSystem',url:canonical,version:'1',status:'active',content:'complete',concept:[{code:'package-code',display:'Activated package concept'}]},
      {resourceType:'ValueSet',url:set,version:'1',status:'active',compose:{include:[{system:canonical,version:'1'}]}},
    ])
    const folder='../.benchmarks/ui-admin/uploads';await mkdir(folder,{recursive:true});const path=`${folder}/package-${Date.now()}.tgz`;await writeFile(path,raw)
    await page.getByLabel('FHIR package file').setInputFiles(path)
    await page.getByRole('button',{name:'Upload and inspect',exact:true}).click()
    await expect(page.getByRole('button',{name:'Start import',exact:true})).toBeVisible()
    await expect(page.locator('main')).toContainText('2 terminology resources')
    await expect(page.getByRole('alert').filter({hasText:'Dependencies are declared'})).toContainText('not installed automatically')
    const jobId=new URL(page.url()).pathname.split('/').at(-1)!
    const inspection=await (await page.request.get(`/api/admin/jobs/${jobId}`)).json()
    expect(inspection.format).toBe('fhir-package');expect(inspection.sha256).toBe(createHash('sha256').update(raw).digest('hex'))
    expect(inspection.inspection.resources).toHaveLength(2);expect(inspection.inspection.records).toBe(2)
    expect((await (await page.request.get(`/api/catalog/CodeSystem?${new URLSearchParams({url:canonical})}`)).json()).total).toBe(0)
    await writeFile(`${folder}/${jobId}-package-inspection.json`,JSON.stringify(inspection,null,2))
    const checkBounds=async()=>expect(await page.evaluate(()=>[...document.querySelectorAll('main,#root,.page,body,header')].filter(el=>el.scrollWidth>el.clientWidth+1).map(el=>el.tagName))).toEqual([])
    if(width === 390) {
      const inventory=page.getByRole('region',{name:'Package resource inventory',exact:true})
      await inventory.focus();await page.keyboard.press('ArrowRight')
      await expect.poll(()=>inventory.evaluate(el=>el.scrollLeft)).toBeGreaterThan(0)
    }
    await checkBounds();await page.screenshot({path:`../.benchmarks/ui-admin/screenshots/package-ready-${width}-${colorScheme}.png`,fullPage:true})
    const codingRequest={resourceType:'Parameters',parameter:[{name:'coding',valueCoding:{system:canonical,version:'1',code:'package-code'}}]}
    expect((await page.request.post('/r4/CodeSystem/$lookup',{data:codingRequest})).status()).toBe(404)
    await page.getByRole('button',{name:'Start import',exact:true}).click()
    await expect(page.getByRole('alert').filter({hasText:'2 resources activated'})).toBeVisible({timeout:20_000})
    const finished=await (await page.request.get(`/api/admin/jobs/${jobId}`)).json()
    expect(finished.result.resources).toHaveLength(2);expect(finished.result.imported).toBe(2)
    const published=await page.request.post('/r4/CodeSystem/$lookup',{data:codingRequest})
    expect(published.status()).toBe(200);expect(JSON.stringify(await published.json())).toContain('Activated package concept')
    const expansion=await page.request.post('/r4/ValueSet/$expand',{data:{resourceType:'Parameters',parameter:[{name:'url',valueUri:set},{name:'count',valueInteger:1}]}})
    expect(expansion.status()).toBe(200);expect((await expansion.json()).expansion.contains[0].code).toBe('package-code')
    await checkBounds();await page.screenshot({path:`../.benchmarks/ui-admin/screenshots/package-succeeded-${width}-${colorScheme}.png`,fullPage:true})
    await page.getByRole('link',{name:canonical,exact:true}).click();await expect(page.locator('main')).toContainText('Activated package concept')
    }
})
