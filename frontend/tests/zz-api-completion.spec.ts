import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { randomUUID } from 'node:crypto'

test('the embedded R4 API executes rich operations on real administrator imports', async ({request,baseURL}) => {
  const origin=baseURL!
  const password=readFileSync(process.env.ADMIN_TEST_PASSWORD_FILE!, 'utf8').trim()
  test.setTimeout(100_000)
  const signIn=()=>request.post('/api/admin/login',{headers:{Origin:origin},data:{username:'admin',password}})
  let login=await signIn()
  if(login.status()===429){
    await expect.poll(async()=>{
      login=await signIn()
      expect([200,429]).toContain(login.status())
      return login.status()
    },{timeout:70_000,intervals:[15_000]}).toBe(200)
  }
  expect(login.status()).toBe(200)
  const session=await (await request.get('/api/admin/session')).json()
  const headers={Origin:origin,'X-CSRF-Token':session.csrfToken,'Content-Type':'application/fhir+json'}
  const system=`urn:term-squid:api:${randomUUID()}`
  const set=system+':set', map=system+':map'
  const resources=[
    {resourceType:'CodeSystem',url:system,version:'1',status:'active',date:'2020-01-01',content:'complete',hierarchyMeaning:'is-a',concept:[{code:'root',property:[{code:'notSelectable',valueBoolean:true}],concept:[{code:'a',display:'Alpha',definition:'Generated definition',designation:[{language:'ru',use:{system:'urn:use',code:'display'},value:'Альфа'},{language:'ru',value:'Синоним'}]},{code:'b',display:'Bravo'},{code:'inactive',property:[{code:'inactive',valueBoolean:true}]}]}]},
    {resourceType:'ValueSet',url:set,version:'1',status:'active',compose:{include:[{system}]}},
    {resourceType:'ValueSet',url:set+':ucum',version:'1',status:'active',compose:{include:[{system:'http://unitsofmeasure.org',concept:[{code:'mg'},{code:'mg/dL'}]}]}},
    {resourceType:'ConceptMap',url:map,version:'1',status:'active',sourceUri:set,group:[{source:system,target:system,sourceVersion:'1',targetVersion:'1',element:[{code:'a',target:[{code:'b',equivalence:'equivalent',dependsOn:[{property:'urn:context',system:'urn:kind',value:'in'}],product:[{property:'urn:result',system:'urn:kind',value:'out'}]}]}]}]},
  ]
  for(const resource of resources){
    const upload=await request.post('/api/admin/jobs',{headers,data:resource})
    expect(upload.status()).toBe(200)
    const job=await upload.json()
    const started=await request.post(`/api/admin/jobs/${job.id}/start`,{headers,data:{}})
    expect(started.ok()).toBe(true)
    await expect.poll(async()=> (await (await request.get(`/api/admin/jobs/${job.id}`)).json()).state).toBe('succeeded')
  }
  const logout=await request.post('/api/admin/logout',{headers,data:{}})
  expect(logout.ok()).toBe(true)
  const parameter=(body: {parameter:{name:string,part?:Record<string,unknown>[],valueBoolean?:boolean,valueString?:string}[]},name:string)=>body.parameter.find(p=>p.name===name)!
  const lookup=await request.get('/r4/CodeSystem/$lookup',{params:{system,version:'1',code:'a',date:'2021',displayLanguage:'ru',property:'designation'}})
  expect(lookup.status()).toBe(200)
  expect(lookup.headers()['content-type']).toBe('application/fhir+json')
  expect(parameter(await lookup.json(),'display').valueString).toBe('Альфа')
  const validation=await request.post('/r4/CodeSystem/$validate-code',{data:{resourceType:'Parameters',parameter:[{name:'url',valueUri:system},{name:'codeableConcept',valueCodeableConcept:{coding:[{system,code:'missing'},{system,version:'1',code:'a',display:'Синоним'}]}},{name:'displayLanguage',valueCode:'ru-RU'},{name:'abstract',valueBoolean:false}]}})
  expect(validation.status()).toBe(200)
  expect(parameter(await validation.json(),'result').valueBoolean).toBe(true)
  const expansion=await request.get('/r4/ValueSet/$expand',{params:{url:set,activeOnly:'true',excludeNotForUI:'true',displayLanguage:'ru',includeDesignations:'true',designation:'ru',filter:'Аль',count:1,'force-system-version':system+'|1'}})
  expect(expansion.status()).toBe(200)
  const expanded=await expansion.json()
  expect(expanded.expansion.total).toBe(1)
  expect(expanded.expansion.contains[0]).toMatchObject({system,version:'1',code:'a',display:'Альфа',designation:[{language:'ru',value:'Альфа'},{language:'ru',value:'Синоним'}]})
  const subsumption=await request.post('/r4/CodeSystem/$subsumes',{data:{resourceType:'Parameters',parameter:[{name:'codingA',valueCoding:{system,version:'1',code:'root'}},{name:'codingB',valueCoding:{system,version:'1',code:'a'}}]}})
  expect(subsumption.status()).toBe(200)
  expect((await subsumption.json()).parameter[0].valueCode).toBe('subsumes')
  const translation=await request.post('/r4/ConceptMap/$translate',{data:{resourceType:'Parameters',parameter:[{name:'url',valueUri:map},{name:'conceptMapVersion',valueString:'1'},{name:'coding',valueCoding:{system,version:'1',code:'a'}},{name:'source',valueUri:set},{name:'dependency',part:[{name:'element',valueUri:'urn:context'},{name:'concept',valueCodeableConcept:{coding:[{system:'urn:kind',code:'in'}]}}]}]}})
  expect(translation.status()).toBe(200)
  const translated=await translation.json()
  expect(parameter(translated,'result').valueBoolean).toBe(true)
  expect(parameter(translated,'match').part).toEqual(expect.arrayContaining([expect.objectContaining({name:'concept',valueCoding:expect.objectContaining({system,version:'1',code:'b'})}),expect.objectContaining({name:'product'}),{name:'source',valueUri:map+'|1'}]))
  const units=await request.get('/r4/ValueSet/$expand',{params:{url:set+':ucum'}})
  expect(units.status()).toBe(200)
  const selectedUnits=await units.json()
  expect(selectedUnits.expansion.total).toBe(2)
  expect(selectedUnits.expansion.contains).toEqual(expect.arrayContaining([
    expect.objectContaining({system:'http://unitsofmeasure.org',version:'2.2',code:'mg',display:'milligram'}),
    expect.objectContaining({system:'http://unitsofmeasure.org',version:'2.2',code:'mg/dL'}),
  ]))
  for(const [code,valid] of [['mg',true],['kg',false]] as const){
    const member=await request.get('/r4/ValueSet/$validate-code',{params:{url:set+':ucum',system:'http://unitsofmeasure.org',code}})
    expect(member.status()).toBe(200)
    expect(parameter(await member.json(),'result').valueBoolean).toBe(valid)
  }
  const capabilities=await request.get('/r4/metadata?mode=terminology')
  expect(capabilities.status()).toBe(200)
  expect((await capabilities.json()).codeSystem).toEqual(expect.arrayContaining([expect.objectContaining({uri:system})]))
})
