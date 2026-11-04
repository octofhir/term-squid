import { test, expect } from '@playwright/test'
const system='http://example.org/term-squid-benchmark/codes'
const valueSet='http://example.org/term-squid-benchmark/subset'
const version='1.0.0'
const input=(code='C00007',display?:string)=>({name:'coding',valueCoding:{system,version,code,...(display?{display}:{}),userSelected:true}})
const params=(...parameter:object[])=>({resourceType:'Parameters',parameter})
const parameter=(body:{parameter:{name:string;valueBoolean?:boolean;valueString?:string}[]},name:string)=>body.parameter.find(p=>p.name===name)

test('R4 public Coding inputs preserve version and errors over the embedded HTTP API',async({request})=>{
  const lookup=await request.post('/r4/CodeSystem/$lookup',{data:params(input())})
  expect(lookup.status()).toBe(200);expect(parameter(await lookup.json(),'display')?.valueString).toBe('Benchmark concept 7')
  for(const [path,selector] of [
    ['/r4/CodeSystem/$validate-code',{name:'url',valueUri:system}],
    ['/r4/ValueSet/$validate-code',{name:'url',valueUri:valueSet}],
    ['/r4/ValueSet/$validate-code',{name:'valueSet',resource:{resourceType:'ValueSet',compose:{include:[{system,concept:[{code:'C00007'}]}]}}}],
  ] as const) {
    const response=await request.post(path,{data:params(selector,input())})
    expect(response.status()).toBe(200);const body=await response.json()
    expect(parameter(body,'result')?.valueBoolean).toBe(true);expect(parameter(body,'version')?.valueString).toBe(version)
  }
  const badDisplay=await request.post('/r4/CodeSystem/$validate-code',{data:params({name:'url',valueUri:system},input('C00007','Wrong display'))})
  expect(badDisplay.status()).toBe(200);expect(parameter(await badDisplay.json(),'result')?.valueBoolean).toBe(false)
  const selected=await (await request.get(`/api/catalog/CodeSystem?${new URLSearchParams({url:system,version})}`)).json()
  const id=selected.items[0].id
  const instance=await request.post(`/r4/CodeSystem/${id}/$lookup`,{data:params(input())})
  expect(instance.status()).toBe(200)
  for(const coding of [{system:'urn:wrong',version,code:'C00007'},{system,version:'wrong',code:'C00007'}]) {
    const response=await request.post(`/r4/CodeSystem/${id}/$validate-code`,{data:params({name:'coding',valueCoding:coding})})
    expect(response.status()).toBe(400);expect((await response.json()).resourceType).toBe('OperationOutcome')
  }
  for(const data of [params(input(),{name:'code',valueCode:'C00007'}),params(input(),{name:'displayLanguage',valueCode:'de_invalid'}),'{']) {
    const response=await request.post('/r4/CodeSystem/$lookup',{headers:{'Content-Type':'application/fhir+json'},data})
    expect(response.status()).toBe(400);expect(response.headers()['content-type']).toContain('application/fhir+json');expect((await response.json()).resourceType).toBe('OperationOutcome')
  }
  const metadata=await (await request.get('/r4/metadata')).json()
  expect(metadata.fhirVersion).toBe('4.0.1')
  expect(metadata.rest[0].resource[0].operation.map((o:{name:string})=>o.name)).toEqual(['lookup','validate-code','subsumes'])
})
