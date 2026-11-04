import { test, expect } from '@playwright/test'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'

const base = 'http://example.org/term-squid-benchmark/'
const parameter = (body: { parameter: { name: string; valueBoolean?: boolean; valueString?: string; valueCode?: string }[] }, name: string) => body.parameter.find(p => p.name === name)

test('anonymous read APIs stay public over GET and POST; errors stay JSON', async ({ request }) => {
  const operations: [string, Record<string,string>, (b: unknown) => void][] = [
    ['CodeSystem/$lookup', { system: `${base}codes`,code:'C00007' }, b => expect(parameter(b as Parameters<typeof parameter>[0],'display')?.valueString).toBe('Benchmark concept 7')],
    ['CodeSystem/$validate-code', { system:`${base}codes`,code:'C00007' }, b => expect(parameter(b as Parameters<typeof parameter>[0],'result')?.valueBoolean).toBe(true)],
    ['ValueSet/$validate-code', { url:`${base}subset`,system:`${base}codes`,code:'C00007' }, b => expect(parameter(b as Parameters<typeof parameter>[0],'result')?.valueBoolean).toBe(true)],
    ['ValueSet/$expand', { url:`${base}subset`,count:'2' }, b => { const value=b as { expansion:{total:number;contains:unknown[]} };expect(value.expansion.total).toBe(255);expect(value.expansion.contains).toHaveLength(2) }],
    ['CodeSystem/$subsumes', { system:`${base}codes`,codeA:'root',codeB:'C00007' }, b => expect(parameter(b as Parameters<typeof parameter>[0],'outcome')?.valueCode).toBe('subsumes')],
    ['ConceptMap/$translate', { url:`${base}map`,system:`${base}codes`,code:'C00007' }, b => expect(parameter(b as Parameters<typeof parameter>[0],'result')?.valueBoolean).toBe(true)],
  ]
  for (const [path,values,check] of operations) for (const method of ['GET','POST']) {
    const response=method==='GET' ? await request.get(`/r4/${path}?${new URLSearchParams(values)}`) : await request.post(`/r4/${path}`,{data:{resourceType:'Parameters',parameter:Object.entries(values).map(([name,value])=>({name,...(name==='count'?{valueInteger:Number(value)}:{valueString:value})}))}})
    expect(response.status(),`${method} ${path}`).toBe(200);check(await response.json())
  }
  const licenses = await request.get('/licenses/THIRD_PARTY.txt')
  expect(licenses.status()).toBe(200); expect(await licenses.text()).toContain('react@19.3.1')
  for (const path of ['/api/unknown','/api/admin/unknown','/r4/unknown']) {
    const response=await request.get(path);expect(response.status()).toBe(404);expect(response.headers()['content-type']).toContain(path.startsWith('/r4/') ? 'application/fhir+json' : 'application/json');const body=await response.json();if(path.startsWith('/r4/')) expect(body.resourceType).toBe('OperationOutcome')
  }
  for (const path of ['/api/admin/jobs','/api/admin/jobs/00000000-0000-0000-0000-000000000000/start']) {
    const response=await request.post(path,{data:'{}'});expect(response.status()).toBe(401)
  }
})

test('public UI deep links, pagination, dialogs and keyboard navigation', async ({ page }) => {
  const errors:string[]=[], external:string[]=[];page.on('pageerror',e=>errors.push(e.message))
  page.on('request',r=>{if(!r.url().startsWith(new URL(process.env.UI_BASE_URL ?? 'http://127.0.0.1:8082').origin)) external.push(r.url())})
  await page.goto('/catalog/CodeSystem')
  await expect(page.getByRole('heading',{name:'Code systems',exact:true})).toBeVisible()
  await expect(page.getByRole('button',{name:'Import resource'})).toHaveCount(0)
  await expect(page.getByRole('button',{name:'Administration',exact:true})).toHaveCount(0)
  await page.getByRole('link',{name:'TermSquidBenchmarkCodes',exact:true}).click()
  await page.getByRole('button',{name:'Versions',exact:true}).click()
  await expect(page.getByRole('dialog',{name:'Business versions'})).toBeVisible()
  await page.keyboard.press('Escape');await expect(page.getByRole('dialog')).toHaveCount(0)
  await page.getByRole('button',{name:'FHIR JSON',exact:true}).click()
  await expect(page.getByRole('dialog',{name:'FHIR JSON'})).toContainText('CodeSystem')
  await page.keyboard.press('Escape')
  await page.getByRole('button',{name:'Next page',exact:true}).click()
  await expect(page.locator('main')).toContainText('26–50 of')
  await page.goto('/operations')
  await page.getByLabel('Canonical URL',{exact:true}).fill(`${base}codes`)
  await page.getByLabel('Code',{exact:true}).fill('C00007')
  await page.getByRole('button',{name:'Run $lookup',exact:true}).click()
  await expect(page.getByRole('region',{name:'Operation result'})).toContainText('Benchmark concept 7')
  await page.goto('/');await page.keyboard.press('Tab')
  await expect(page.getByRole('link',{name:'Skip to content',exact:true})).toBeFocused()
  expect(errors).toEqual([]); expect(external).toEqual([])
})

async function login(page: import('@playwright/test').Page, destination='/admin') {
  await page.goto(destination);await expect(page).toHaveURL(/\/login\?returnTo=/)
  await page.getByLabel('Username',{exact:true}).fill('admin')
  const password=(await readFile(process.env.ADMIN_TEST_PASSWORD_FILE!, 'utf8')).trim()
  await page.getByLabel('Password',{exact:true}).fill(password)
  await page.locator('main form').getByRole('button',{name:'Sign in',exact:true}).click()
}
test('real upload inspects, activates and becomes publicly usable; logout revokes session', async ({ page }) => {
  test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE,'Requires the local test administrator')
  await login(page)
  await expect(page.getByRole('heading',{name:'Administration',exact:true})).toBeVisible()
  const canonical=`urn:term-squid:browser:${Date.now()}`
  const content={resourceType:'CodeSystem',url:canonical,version:'1.0.0',name:'BrowserImportFixture',status:'active',content:'complete',concept:[{code:'ui-code',display:'Activated from the browser'}]}
  const raw=JSON.stringify(content,null,2), folder='../.benchmarks/ui-admin/uploads'
  await mkdir(folder,{recursive:true});const file=`${folder}/browser-${Date.now()}.json`;await writeFile(file,raw)
  await page.getByLabel('FHIR JSON file').setInputFiles(file)
  await page.getByRole('button',{name:'Upload and inspect',exact:true}).click()
  await expect(page.getByRole('button',{name:'Start import',exact:true})).toBeVisible()
  const jobId=new URL(page.url()).pathname.split('/').at(-1)!
  const inspection=await (await page.request.get(`/api/admin/jobs/${jobId}`)).json()
  expect(inspection.sha256).toBe(createHash('sha256').update(raw).digest('hex'))
  expect(inspection.bytes).toBe(Buffer.byteLength(raw));expect(inspection.inspection.records).toBe(1)
  await writeFile(`${folder}/${jobId}-inspection.json`,JSON.stringify(inspection,null,2))
  expect((await (await page.request.get(`/api/catalog/CodeSystem?${new URLSearchParams({url:canonical})}`)).json()).total).toBe(0)
  await page.getByRole('button',{name:'Start import',exact:true}).click()
  await page.getByRole('link',{name:'Browse activated resource →',exact:true}).click({timeout:20_000})
  await expect(page.locator('main')).toContainText('Activated from the browser')
  const lookup=await page.request.post('/r4/CodeSystem/$lookup',{data:{resourceType:'Parameters',parameter:[{name:'system',valueUri:canonical},{name:'code',valueCode:'ui-code'}]}})
  expect(lookup.status()).toBe(200)
  await page.getByRole('button',{name:'Account menu',exact:true}).click()
  await page.getByRole('menuitem',{name:'Sign out',exact:true}).click()
  await page.getByRole('dialog',{name:'Sign out',exact:true}).getByRole('button',{name:'Sign out',exact:true}).click()
  await expect(page.getByRole('banner').getByRole('button',{name:'Sign in',exact:true})).toBeVisible()
  expect((await page.request.get('/api/admin/jobs')).status()).toBe(401)
  await expect(page.getByRole('button',{name:'Import resource'})).toHaveCount(0)
})
test('session expiry removes admin controls and protects direct APIs', async ({ page }) => {
  test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE || process.env.TEST_SHORT_SESSION !== '1','Requires a 30-second local test session')
  test.setTimeout(50_000)
  await login(page)
  await expect(page.getByRole('heading',{name:'Administration',exact:true})).toBeVisible()
  await expect(page.getByRole('banner').getByRole('button',{name:'Sign in',exact:true})).toBeVisible({timeout:35_000})
  await expect(page.getByRole('button',{name:'Upload and inspect'})).toHaveCount(0)
  expect((await page.request.post('/api/admin/jobs',{data:'invalid'})).status()).toBe(401)
})

test('authentication failure preserves public browsing and never flashes import controls', async ({ page }) => {
  let requests=0
  await page.route('**/api/admin/session', async route => { requests++; await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'Unavailable'}})}) })
  await page.goto('/catalog/CodeSystem')
  await expect(page.getByRole('link',{name:'TermSquidBenchmarkCodes',exact:true})).toBeVisible()
  await expect(page.getByRole('button',{name:'Import resource'})).toHaveCount(0)
  await page.getByRole('banner').getByRole('button',{name:'Sign in',exact:true}).click()
  await expect(page.getByRole('alert')).toContainText('Administration is temporarily unavailable')
  expect(requests).toBeLessThanOrEqual(2)
})

test('a delayed session response cannot restore controls after logout', async ({ page }) => {
  test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE,'Requires the local test administrator')
  await login(page);await expect(page.getByRole('heading',{name:'Administration',exact:true})).toBeVisible()
  const previous=await (await page.request.get('/api/admin/session')).json()
  let delayed: import('@playwright/test').Route | undefined
  await page.route('**/api/admin/session', async route => {
    if (!delayed) delayed=route
    else await route.continue()
  })
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')))
  await expect.poll(()=>!!delayed).toBe(true)
  await page.getByRole('button',{name:'Account menu',exact:true}).click()
  await page.getByRole('menuitem',{name:'Sign out',exact:true}).click()
  await page.getByRole('dialog',{name:'Sign out',exact:true}).getByRole('button',{name:'Sign out',exact:true}).click()
  await expect(page.getByRole('banner').getByRole('button',{name:'Sign in',exact:true})).toBeVisible()
  await delayed!.fulfill({status:200,contentType:'application/json',body:JSON.stringify(previous)})
  await page.waitForLoadState('networkidle')
  await expect(page.getByRole('button',{name:'Administration',exact:true})).toHaveCount(0)
  await expect(page.getByRole('button',{name:'Account menu',exact:true})).toHaveCount(0)
})

test('login normalizes and rejects traversal in the return destination', async ({ page }) => {
  test.skip(!process.env.ADMIN_TEST_PASSWORD_FILE,'Requires the local test administrator')
  const external:string[]=[]
  await page.route('**/*',async route=>{
    if(new URL(route.request().url()).origin!==new URL(process.env.UI_BASE_URL!).origin) { external.push(route.request().url());await route.abort() }
    else await route.continue()
  })
  await login(page,`/login?${new URLSearchParams({returnTo:'/admin/..//evil.example'})}`)
  await expect(page.getByRole('heading',{name:'Administration',exact:true})).toBeVisible()
  expect(new URL(page.url()).pathname).toBe('/admin');expect(external).toEqual([])
})
