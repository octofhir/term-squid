# Local FHIRsmith comparison after the R4 API delivery

November 4, 2026. This comparison uses the delivered term-squid 0.1.3 backend from `88b65c42f6f495efd050ddc2c22470560c7c9d17` and local FHIRsmith 0.18.2 / txVersion 1.10.1 from `25a41f87711507789b361480ee4efae42df504d4`. Only loopback endpoints receive requests. This report covers generated finite resources and selected inline/UCUM workflows, not the complete clinical corpus or unrestricted FHIR conformance.

The November 5 continuation is recorded separately in [REQUEST_CONTEXT_PERFORMANCE.md](REQUEST_CONTEXT_PERFORMANCE.md), [CONTEXT_MIXED_REPORT.md](CONTEXT_MIXED_REPORT.md) and [ECOSYSTEM_GAP_AUDIT.md](ECOSYSTEM_GAP_AUDIT.md). This report preserves the original November 4 artifact and evidence.

## Functional comparison

The unchanged seven local gates pass **7/7 on term-squid and 6/7 on FHIRsmith**. FHIRsmith's positive reverse translation returns the original target in `match.concept`, rather than the expected original source code. Its extra `sourceConcept` does contain the original source, but the checker still requires the requested translation outcome in `match.concept`. The failed scenario receives no comparative speed score.

The new reusable [differential driver](../benchmarks/compare-api.ts) retains all 17 existing preflight probes and adds Coding/CodeableConcept, inline resources, POST operations, language, text-filter, UCUM and error checks. **44/44 selected expectations pass on term-squid; 34/44 pass on FHIRsmith** after configuring UCUM on both sides. These are selected term-squid contract expectations, not an independent certification score. Full responses and requests are in `.benchmarks/fhirsmith-comparison-20261104/api-ucum/responses.json`.

| Difference | Observed FHIRsmith behavior | Interpretation |
| --- | --- | --- |
| Reverse translation, primitive and Coding | HTTP 200/result=true, but `match.concept` retains the original target | Direction/output mismatch against the unchanged reverse gate; unscored |
| CodeSystem CodeableConcept with missing and valid candidates | result=false, although the valid candidate's display/version are returned | Candidate aggregation differs from the agreed term-squid any-valid contract; retain diagnostics for independent ecosystem adjudication |
| Inline CodeSystem, three primitive variants | HTTP 400, unable to identify the code context | Optional input capability/context handling differs |
| Inline CodeSystem with explicit Coding context | result=false with an unknown-system diagnostic | The supplied inline resource is not used by this reference path |
| Text-filtered expansion, GET and POST | Multiword display filter returns 256 entries and includes excluded C00042; unfiltered expansion returns the correct 255; exact code filter returns one entry | Search policies differ, and the filtered result also changes excluded membership; record both requests rather than assuming identical text-search algorithms |
| CodeableConcept translation with missing then mapped candidate | result=false, no translations | Candidate selection differs; not a mandatory all-candidates coverage claim |

Ordinary lookup/validation, included/excluded/outside/missing membership, all four subsumption outcomes, paged expansion, POST Coding, inline ValueSet composition and an explicitly targeted inline ConceptMap pass on both servers. UCUM `mg`, `mm[Hg]`, `mg/dL` and an invalid expression pass on both with model 2.2 loaded. Unknown lookup versions and missing system context produce FHIR errors on both.

FHIR R4 explicitly permits servers to decline inline CodeSystems/ConceptMaps. Its expansion text-filter algorithm is delegated to the server. CodeSystem validation describes any-valid CodeableConcept handling, while translation permits server selection of candidate codings. These distinctions prevent treating every reference difference as a specification defect: see [CodeSystem validation](https://hl7.org/fhir/R4/codesystem-operation-validate-code.html), [expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html) and [translation](https://hl7.org/fhir/R4/conceptmap-operation-translate.html).

The initial retained FHIRsmith library did not configure UCUM. That configuration failed positive UCUM checks and could misleadingly return false for an invalid unit because the whole system was unknown. The final run uses the reference's documented `ucum:` source with the real 2.2 essence file; unavailable-system negatives are explicitly rejected by the new driver. The initial responses remain available as diagnostics, not the final comparison.

## Measurement setup

- Hardware actually observed in this run: **Apple M1, eight cores, 16 GiB, macOS 26.5**. Earlier reports on other hosts are not substituted.
- PostgreSQL runs in Docker; the VM exposes eight CPUs and 8,393,605,120 bytes of memory. term-squid uses explicit pool 32; FHIRsmith uses its default caching/provider configuration. This compares the local deployments, not language-only execution or whole-system resource efficiency.
- Node 24.16.0, Bun 1.4.3, k6 2.3.0. Both binaries/processes run natively. UI preview and unrelated idle services remain available, so this is a nondedicated host.
- Matching synthetic resources version 1.0.0, fingerprint `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`, plus R4 core 4.0.1 and UCUM model 2.2. term-squid's documented legacy-v2 import normalization remains. No native clinical release is loaded for this comparison.
- Six passing common scenarios: CodeSystem validation, lookup, ValueSet membership, expansion, subsumption and forward translation. Changing codes, negative results, exclusions, version and page checks remain unchanged.
- Fresh processes run sequentially: FHIRsmith then term-squid for the main matrix, followed by term-squid then FHIRsmith for the loaded lookup/subsumption repeat. Main windows are 30 seconds at 1/10/50 VU, with a discarded five-second per-operation warmup. The repeat uses 50 VU. Every measured response is checked. Builds and browser validation finish before measurement.
- Server CPU time and RSS are sampled once per second through the existing process observer. PostgreSQL/VM/client memory are excluded from the server-process RSS figures.

The FHIRsmith source checkout is clean at the pinned revision. Comparing every tracked source file with the executable archive found only the existing bootstrap change: resolve R4 core as `hl7.fhir.r4.core#4.0.1` instead of an unversioned registry lookup. No request handler/provider/worker code is patched. The UCUM XML and run-specific configuration stay under `.benchmarks/`; the old reference configuration is preserved.

## Matched performance results

All **40 measurement windows** pass **10,795,783 response checks**, with zero HTTP errors and zero failed checks. The table contains twenty paired points: eighteen in the primary matrix and two in the reverse-order repeat. Ratio is term-squid throughput divided by FHIRsmith throughput. p50/p95/average latency remain in the raw JSON alongside p99.

| Operation | VU | term-squid req/s | FHIRsmith req/s | Ratio | term-squid p99 ms | FHIRsmith p99 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| main TS-CM01 | 1 | 3,105.9 | 4,372.2 | 0.710x | 0.793 | 0.445 |
| main TS-CM01 | 10 | 15,021.6 | 8,085.3 | 1.858x | 1.438 | 2.801 |
| main TS-CM01 | 50 | 22,540.7 | 7,654.4 | 2.945x | 9.705 | 13.858 |
| main TS-CS01 | 1 | 3,723.2 | 3,683.7 | 1.011x | 0.341 | 0.399 |
| main TS-CS01 | 10 | 15,057.5 | 5,495.2 | 2.740x | 1.658 | 3.976 |
| main TS-CS01 | 50 | 23,174.1 | 5,507.6 | 4.208x | 9.981 | 17.909 |
| main TS-EX01 | 1 | 1,460.4 | 614.5 | 2.377x | 0.817 | 4.679 |
| main TS-EX01 | 10 | 5,749.3 | 866.2 | 6.637x | 2.703 | 18.098 |
| main TS-EX01 | 50 | 5,483.0 | 830.7 | 6.600x | 34.952 | 104.238 |
| main TS-LK01 | 1 | 3,369.6 | 4,679.5 | 0.720x | 0.598 | 0.505 |
| main TS-LK01 | 10 | 14,187.3 | 4,047.2 | 3.505x | 1.668 | 21.987 |
| main TS-LK01 | 50 | 23,002.0 | 2,385.8 | 9.641x | 9.894 | 79.266 |
| main TS-SS01 | 1 | 3,411.9 | 5,325.2 | 0.641x | 0.701 | 0.477 |
| main TS-SS01 | 10 | 13,460.8 | 10,243.1 | 1.314x | 2.060 | 3.119 |
| main TS-SS01 | 50 | 21,983.1 | 10,648.5 | 2.064x | 10.456 | 12.614 |
| main TS-VC01 | 1 | 3,590.9 | 389.0 | 9.232x | 0.440 | 11.083 |
| main TS-VC01 | 10 | 14,111.1 | 1,107.2 | 12.745x | 1.888 | 44.311 |
| main TS-VC01 | 50 | 24,121.0 | 2,186.7 | 11.031x | 8.808 | 81.941 |
| repeat TS-LK01 | 50 | 22,295.1 | 9,350.5 | 2.384x | 10.390 | 12.922 |
| repeat TS-SS01 | 50 | 22,889.0 | 10,590.1 | 2.161x | 10.078 | 11.755 |

At 50 VU term-squid wins all six admitted common scenarios, with lower p99 in each paired point. This does not establish an overall 3x result. At 1 VU lookup, subsumption and forward translation still lose both throughput and p99; CodeSystem validation is near parity. Expansion throughput peaks at 10 VU and falls from 5,749 to 5,483 req/s at 50 VU while p99 rises from 2.703 to 34.952 ms. These are concrete remaining optimization targets.

The lookup reference is strongly order/state sensitive: **2,385.8 primary versus 9,350.5 repeat req/s**, while term-squid remains **23,002.0 versus 22,295.1**. Consequently the paired gain ranges from **9.641x to 2.384x**; the larger figure is not a stable capacity claim. The repeat preserves a gain, but two sequential readings are neither confidence intervals nor proof of the cause. Subsumption is steadier: 2.064x primary / 2.161x repeat. No result is discarded or replaced with a favorable historical value.

Server-process observations across each full group, including warmup/preflight:

| Group | Process | Mean CPU % | Sampled peak RSS MiB |
| --- | --- | ---: | ---: |
| main | term-squid | 65.5 | 80.4 |
| main | fhirsmith | 84.5 | 624.8 |
| repeat | term-squid | 81.6 | 39.7 |
| repeat | fhirsmith | 98.5 | 452.9 |

CPU 100% represents one core. RSS is sampled, not a measured whole-process peak, and excludes the term-squid PostgreSQL process, Docker VM, client and other services. These figures do not prove whole-deployment memory or CPU savings. The summary and process time series retain the measurement scope.

The measured backend SHA-256 is `7abcb42ceb8f148f3e814af8b43f4ac21e5a02fdc227196f9f4e9cba66fcb86d`; the UCUM XML SHA-256 is `dfccea1b5dc284245ebae97edd1dc03c45864da4e87df55bc9851797b4fd0b61`. Both participants use the same unchanged local harness fingerprint `2776d35b99210b9f3c73865518be5b840295b754157b2e2efd945d4f2296e6cd`. `summary.json` validates matching dataset/environment/protocol identities, successful manifests and every scored result before deriving this table.

Decoded UTF-8 response sizes are also retained in `response-sizes.json`, using the same changing-code sampler at index 1. term-squid/FHIRsmith bytes are validation 177/291, lookup 297/921, membership 178/292, expansion 11,561/5,881, subsumption 88/88 and forward translation 367/556. Payloads are not byte identical: optional diagnostics/metadata/definition content differ, while the shared mandatory semantic checks are unchanged. In particular, the expansion result is faster despite a larger decoded response in this sample. These sizes are examples, not measured wire-byte distributions.

## Fresh upstream coverage audit and target compatibility

After timing, the original pinned twenty-scenario upstream preflight is repeated on fresh local processes. term-squid passes **2/20** (`FS01`, `LK03`); FHIRsmith passes **3/20** (also `CM02`). Failures/skips remain visible, and `LK05` cannot earn a positive score without the missing positive SNOMED context. Neither deployment has the full clinical corpus, so this audit produces no clinical speed score or complete-corpus ranking.

The extra `CM02` pass exposes a specific compatibility difference rather than missing standard translation. The unchanged upstream helper writes a CodeSystem URI into `target`. R4 defines `target` as a ValueSet constraint and `targetsystem` as the CodeSystem selector. A separate four-request diagnostic verifies:

| Request selector for the R4 core administrative-gender map | term-squid | FHIRsmith |
| --- | --- | --- |
| `target=http://terminology.hl7.org/CodeSystem/v3-AdministrativeGender` | HTTP 404, ValueSet unavailable | HTTP 200, male → M |
| `targetsystem=http://terminology.hl7.org/CodeSystem/v3-AdministrativeGender` | HTTP 200, male → M | HTTP 200, male → M |

`target-diagnostic.json` retains both exact requests and complete responses. The comparison leaves the original upstream helper, checks and scores untouched. term-squid retains standard R4 target-ValueSet semantics; no silent system-URI alias is introduced to inflate coverage. Compatibility with this benchmark convention remains an explicit product decision. The standard request passing separately does not turn the unchanged upstream `CM02` into a term-squid pass. See the [R4 translation parameter definitions](https://hl7.org/fhir/R4/conceptmap-operation-translate.html).

## Reproduction and limits

Run both local servers with the matching resources, then execute:

```sh
bun test benchmarks
bun benchmarks/compare-api.ts --left http://127.0.0.1:18081/r4 \
  --right http://127.0.0.1:17003/tx/r4 \
  --output .benchmarks/UNIQUE-api-comparison

# Repeat with --name fhirsmith and its URL/revision, using fresh processes.
bun benchmarks/bench.ts run --name term-squid --url http://127.0.0.1:18081/r4 \
  --server-revision 88b65c42f6f495efd050ddc2c22470560c7c9d17 \
  --tests TS-CS01,TS-LK01,TS-VC01,TS-EX01,TS-SS01,TS-CM01 \
  --vus 1,10,50 --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 + UCUM#2.2'
```

Raw requests/responses, orchestration, settings, logs and process samples remain under `.benchmarks/fhirsmith-comparison-20261104/`; benchmark matrices use `.benchmarks/results/api013-compare-{main,repeat}/{term-squid,fhirsmith}/`. The driver exits unsuccessfully for a failed term-squid expectation; reference differences are retained as evidence. Two additional harness tests reject false positive/malformed results and preserve reverse/error/inline gates; **31 harness tests / 34,235 assertions** pass. The original benchmark assertions and seven gates are unchanged.

This is not the HealthSamurai clinical composite: full SNOMED/LOINC/RxNorm content and the matched twenty-scenario cohort remain separate work. No overall 3x, clinical coverage or first-place claim follows from this finite comparison. No commit, push, package publication or deployment is part of this comparison request.
