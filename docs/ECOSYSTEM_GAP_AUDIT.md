# Independent R4 ecosystem gap audit

November 5, 2026. The agreed [term-squid contract](API_REQUIREMENTS.md) contains 70 top-level inputs across six R4 operations. Its implementation is not equivalent to passing the entire HL7 Terminology Ecosystem test package. This audit keeps those two scopes explicit and preserves every original request, expected response and assertion.

## Verified results

| Profile | term-squid | Pinned local FHIRsmith | Scope |
| --- | ---: | ---: | --- |
| Original complete `case` suite | 6/6 | 6/6 | Original request-local setup; term-squid inventory stays empty |
| Preloaded complete `case` suite | 6/6 | Historical reference run retained | Same official setup preloaded through shared ingestion |
| Selected product API plus explicit UCUM lists | 49/49 | 39/49 | Product expectations; not an independent certification ranking |
| General-mode R4-applicable selection | 110/689 | 682/689 | Unchanged official runner and matcher; all failures retained |

term-squid repeats original/preloaded suites and the complete general audit with prepared retention enabled and disabled. All 689 general-mode pass/fail statuses agree between the two modes, with zero ignored tests. The seven unchanged local FHIR gates also pass in both modes. The general audit exits unsuccessfully; its failures are not converted into ignored cases or admitted speed scores.

Both participants load the complete official R4 core 4.0.1 package and unchanged synthetic package, with real UCUM 2.2 configured. This matches package inputs, not every built-in provider or clinical release. No complete SNOMED/LOINC/RxNorm corpus is installed for this comparison. FHIRsmith's additional built-in providers and client-extension support remain part of its observed capability inventory.

## First failure inventory

The following counts classify the **first matcher failure** of each failing term-squid test. They do not prove that all remaining assertions in that test pass, or distinguish every valid output choice from a semantic defect. Additional per-concept business-version output is legal R4 and remains present; established primitive responses are not reordered or stripped solely to improve a matcher score.

| First matcher failure | Tests |
| --- | ---: |
| array item count differs at .parameter | 107 |
| Response Code fail: should be '2xx' but is '400' | 90 |
| Response Code fail: should be '2xx' but is '404' | 78 |
| Response Code fail: should be '2xx' but is '422' | 61 |
| properties differ at .issue[0]: missing property details | 38 |
| properties differ at .expansion.contains[0]: unexpected property version | 28 |
| string property values differ at .parameter[1].name | 27 |
| string property values differ at .compose.include[0].system | 26 |
| string property values differ at .issue[0].code | 20 |
| string property values differ at .parameter[2].resource.issue[0].details.text | 16 |
| Other first failures | 88 |

The full inventory and complete runner responses remain under `.benchmarks/overnight-20261105/final-gap-analysis.json` and `core-matched-graph-{cache-true,cache-false,matched-fhirsmith}/`. Many mismatches concern output cardinality/shape, optional diagnostics, terminology availability, version-selection extensions or later-version inputs sent to the R4 adapter. They cannot all be described as one missing feature, nor can they all be dismissed as formatting.

The reference's seven failures are the two property-expansion cases, `simple-lookup-1`, `notSelectable-unprop-true`, `translate-2b`, `translate-5` and `translate-5b`. Those failures remain visible alongside its 682 passes.

## Complete suite inventory

This is the same 689-case selection grouped by its original suites, not a new score or a diagnosis of every failure. Every case remains selected. The table helps locate the next concrete API investigation; first-failure messages can mask further differences.

| Official suite | term-squid passes | FHIRsmith passes | Selected tests |
| --- | ---: | ---: | ---: |
| metadata | 0 | 2 | 2 |
| properties | 0 | 0 | 2 |
| simple-cases | 16 | 35 | 36 |
| parameters | 0 | 35 | 35 |
| language | 4 | 26 | 26 |
| language2 | 10 | 25 | 25 |
| extensions | 0 | 12 | 12 |
| validation | 8 | 77 | 77 |
| version | 9 | 206 | 206 |
| overload | 10 | 29 | 29 |
| fragment | 0 | 7 | 7 |
| big | 2 | 5 | 5 |
| other | 0 | 3 | 3 |
| errors | 0 | 7 | 7 |
| deprecated | 0 | 11 | 11 |
| notSelectable | 30 | 49 | 50 |
| inactive | 0 | 12 | 12 |
| case | 6 | 6 | 6 |
| translate | 0 | 9 | 10 |
| translate2 | 0 | 6 | 8 |
| tho | 0 | 32 | 32 |
| exclude | 2 | 8 | 8 |
| search | 4 | 6 | 6 |
| default-valueset-version | 3 | 12 | 12 |
| batch | 0 | 2 | 2 |
| permutations | 6 | 56 | 56 |
| regex-bad | 0 | 4 | 4 |

Representative entry points are `version/version-simple-code-bad-version1` (unknown-edition response policy), `validation/validation-simple-child-of-child` (additional validation inputs), `language/language-echo-en-en-param` (parameter echo), `translate/translate-1` (client protocol/parameter admission) and `fragment/fragment-expansion` (partial-source provider semantics). Inspect the original request, actual body and expected body before assigning a fix. A matcher disagreement does not authorize changing R4 target/version meaning, discarding legal response data or presenting partial content as complete.

## Defects fixed without changing the matcher

The audit exposed mandatory ValueSet supplements being silently ignored. Missing dependencies now fail explicitly across expansion and primitive/Coding/CodeableConcept membership, including imported, contained, temporary and native definitions. Actual selected supplement editions and active status are checked in the same source snapshot. Independent regression tests publish a real overlay, observe successful activation, then verify newer-edition and retirement behavior. Those four official cases still fail the runner's detailed OperationOutcome expectations; fixing the successful-but-incomplete operation does not manufacture extra passes.

Finite R4 property/status semantics, bounded filters, graph preparation, expansion provenance, richer object validation, contained scope and explicit UCUM composition are recorded in [REQUEST_CONTEXT_REPORT.md](REQUEST_CONTEXT_REPORT.md) and [UCUM_VALUESET_REPORT.md](UCUM_VALUESET_REPORT.md). They raise independently observed coverage from earlier retained checkpoints while preserving conservative unsupported errors and the primitive wire contract.

## Next implementation priorities

1. Review failing cases against the declared contract and normative R4 semantics individually, retaining complete request/response evidence. Specify an explicit compatibility profile before changing primitive output or accepting later-version parameters.
2. Expand actual provider/content coverage, including declared external dependencies, built-in terminology families and the complete licensed clinical releases. Unknown terminology must not earn a valid negative-code result.
3. Complete the selected version/language/supplement semantics before advertising broader extensions. Exact required supplement editions currently must match the existing latest/date-based selection; forcing an older supplement edition remains an explicit limitation.
4. Add independent passing cases to CI as each semantic increment is implemented. Keep the complete failing audit available as a gap inventory; do not weaken its goldens or the original seven gates.
5. Resolve the independently reproduced [Unicode comparison boundary](REQUEST_CONTEXT_REPORT.md#remaining-unicode-comparison-boundary): stored and request-local lookup disagree on `U+0130`/`U+0069`/`U+0069 U+0307` under the current SQL/engine normalization policies. The exact older and final artifacts agree on this limitation; plan a shared policy and projection compatibility before changing it.
6. Keep `$closure`, `$find-matches`, batch/compare extensions and unrestricted R5/R6 claims outside the completed six-operation contract until their own scope and implementation are defined.

## Provenance and reproduction

The suite is published package **1.9.6** (its internal TestScript history still identifies 1.9.3), SHA-256 `704c80fae9a3b473a217fbdf751bb9ab29aa0a629b8e8221aad4af590ed32643`; Java runner **6.10.4**, SHA-256 `1106b9d58f9e363e47bea7c4fc065841e5fc91fe9d062775c3bfdd212bd653cc`. [The committed lock](../benchmarks/ecosystem/lock.json) verifies these artifacts. FHIRsmith is `25a41f87711507789b361480ee4efae42df504d4`; its source audit preserves only the documented bootstrap R4 package pin.

The measured term-squid artifact is SHA-256 `6ddb9ad1d0ccb5a62c6a56cbf28ec7d7bc567ddffdbd03dbb1f11d1510cb0ae0`, built from the independent clean source export after frontend generation. Raw summary `matched-graph-current-summary.json` verifies unchanged published inventory: 1,064 CodeSystems, 1,317 ValueSets and 81 ConceptMaps with matching before/after content fingerprints. Source/job details remain private; this public audit creates no persisted temporary definitions.

```sh
python3 -B benchmarks/ecosystem.py run --setup inline --suite all \
  --url http://127.0.0.1:18081/r4 --output .benchmarks/UNIQUE-general-r4
```

Repeat on a fresh local process with `TERMINOLOGY_PREPARED_CACHE=false`, then on the pinned local reference with the same package inputs. Record each manifest and retain expected/actual responses. This semantic audit is separate from load windows, generated isolation diagnostics and the original HealthSamurai composite.
