# Request-context continuation: matched terminology performance

November 5, 2026. Three source-frozen term-squid builds are compared with pinned local FHIRsmith using the unchanged common finite scenarios. All **120 measurement windows** pass **37,208,534 response checks**, with zero HTTP errors and zero failed checks. The final artifact wins the six common loaded scenarios in this cohort; low-concurrency losses and preparation limits remain. This is generated/package-content evidence, not a clinical composite or an overall 3x claim.

## Protocol and source identity

Each cohort uses fresh native processes sequentially: FHIRsmith first for six operations at 1/10/50 VU, then term-squid first for the reverse-order 50-VU lookup/subsumption repeat. Windows are 30 seconds, with one discarded five-second warmup at 1 VU per operation and no inter-test pause. These are the existing local-suite settings, not the original upstream Round 0 timing/scoring protocol. Every response is checked, including changing codes, misses, business versions, exclusions and page contents. Reverse translation remains an explicit semantic reference mismatch and receives no comparative speed score.

The host is Apple M1 / eight cores / 16 GiB / macOS 26.5, with Node 24.16.0, Bun 1.4.3 and k6 2.3.0. PostgreSQL 17.11 runs in the existing Docker VM exposing eight CPUs and 8,393,605,120 bytes. term-squid uses pool 32; FHIRsmith keeps its default provider/cache configuration. The UI preview and idle services remain available: this is a nondedicated host. Builds, automated browsers, correctness audits and profiles finish before load windows.

Both participants have the unchanged synthetic 1.0.0 package, complete R4 core 4.0.1 and UCUM model 2.2. term-squid retains the documented legacy-v2 canonical normalization. The matched database contains 1,064 CodeSystems, 1,317 ValueSets and 81 ConceptMaps, not a complete clinical corpus. No corpus mutation occurs during measurement. Original synthetic SHA-256 is `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`; unchanged local harness is `2776d35b99210b9f3c73865518be5b840295b754157b2e2efd945d4f2296e6cd`.

FHIRsmith is **0.18.2 / txVersion 1.10.1**, source `25a41f87711507789b361480ee4efae42df504d4`. Its source archive SHA-256 is `c29f1fc1f45ad0fb22f21476648f89121a347d2468646208b984d1b41179c8cf`; auditing 831 source files finds only the existing bootstrap R4 package pin, no provider/request/worker changes. UCUM essence SHA-256 is `dfccea1b5dc284245ebae97edd1dc03c45864da4e87df55bc9851797b4fd0b61`.

| term-squid checkpoint | Binary SHA-256 | Exact compilation-input archive SHA-256 |
| --- | --- | --- |
| 200-test prepared-source baseline | `50987ff00778d3a232be3a757a97f572ef1b9cc18420ebbc94c2bbdd0ebc7e9c` | `4729156b45ab209e91300effa483e92972b74e078616d312ed83a2cfadbce02c` |
| 205-test generation/required-supplement checkpoint | `06c0cc8bd50d554da56ae6ebc3f915b1e37224dcd4b292cba7fdd03e65a14669` | `751558f1e871bc814cb8d252c4a0c747ba7ad845fbe71e1bc00c61e53970a5a0` |
| Final 206-test graph/JSON checkpoint | `6ddb9ad1d0ccb5a62c6a56cbf28ec7d7bc567ddffdbd03dbb1f11d1510cb0ae0` | `395562861a056d6420139fe3c395cf45ff52adf6b983c2e38d0e5185bb449492` |

Each archive verifies every one of the 181 compilation/embedded inputs against its frozen manifest. Artifacts use unique immutable filenames. Source changes include mandatory correctness constraints; do not interpret every inter-cohort difference as the isolated effect of one optimization.

## Final matched matrix

The final cohort alone passes **12,422,899 response checks** in forty valid windows. Ratios are final term-squid throughput divided by its freshly matched reference.

| Group / operation | VU | term-squid req/s | FHIRsmith req/s | Ratio | term-squid p99 ms | FHIRsmith p99 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| main TS-CS01 | 1 | 3,950.9 | 3,273.7 | 1.207x | 0.296 | 0.387 |
| main TS-CS01 | 10 | 16,314.9 | 5,169.6 | 3.156x | 1.363 | 3.961 |
| main TS-CS01 | 50 | 28,027.6 | 5,181.4 | 5.409x | 7.009 | 18.277 |
| main TS-LK01 | 1 | 3,806.3 | 4,879.4 | 0.780x | 0.303 | 0.316 |
| main TS-LK01 | 10 | 15,725.0 | 9,391.5 | 1.674x | 1.405 | 3.348 |
| main TS-LK01 | 50 | 25,379.4 | 9,378.9 | 2.706x | 8.072 | 13.120 |
| main TS-VC01 | 1 | 3,781.9 | 2,222.3 | 1.702x | 0.414 | 0.882 |
| main TS-VC01 | 10 | 16,206.7 | 3,726.2 | 4.349x | 1.350 | 6.228 |
| main TS-VC01 | 50 | 27,833.3 | 3,729.7 | 7.463x | 6.909 | 25.263 |
| main TS-EX01 | 1 | 1,569.9 | 711.2 | 2.207x | 0.546 | 1.537 |
| main TS-EX01 | 10 | 6,499.5 | 888.2 | 7.317x | 1.862 | 16.654 |
| main TS-EX01 | 50 | 6,405.5 | 880.9 | 7.272x | 29.763 | 98.092 |
| main TS-SS01 | 1 | 3,729.2 | 5,737.1 | 0.650x | 0.324 | 0.324 |
| main TS-SS01 | 10 | 14,895.6 | 10,972.3 | 1.358x | 1.550 | 2.914 |
| main TS-SS01 | 50 | 25,916.3 | 11,147.5 | 2.325x | 7.777 | 11.696 |
| main TS-CM01 | 1 | 3,709.8 | 4,518.1 | 0.821x | 0.323 | 0.335 |
| main TS-CM01 | 10 | 15,240.0 | 8,102.0 | 1.881x | 1.340 | 2.612 |
| main TS-CM01 | 50 | 24,892.3 | 8,056.5 | 3.090x | 7.817 | 12.398 |
| repeat TS-LK01 | 50 | 25,400.0 | 9,754.9 | 2.604x | 8.042 | 13.028 |
| repeat TS-SS01 | 50 | 25,821.9 | 11,241.6 | 2.297x | 8.061 | 11.562 |

Single-client lookup, subsumption and forward translation still lose to the reference. Expansion retains a concurrency plateau and rising loaded tail latency. No weighted total is calculated, no unsupported clinical scenario receives a positive score, and these points do not meet the overall first-place/3x product goal. Read the raw p50/p95/average values alongside p99 before defining a deployment target.

## Retained earlier cohorts

Both earlier matched cohorts remain visible, including their own reference variation. Before/final deltas are sequential observations on this host, not confidence intervals or a causal CPU decomposition.

| Group / operation | VU | Baseline 200 req/s | Baseline reference | Intermediate 205 req/s | Intermediate reference | Final 206 req/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| main TS-CS01 | 1 | 3,838.0 | 3,775.5 | 3,959.2 | 3,864.4 | 3,950.9 |
| main TS-CS01 | 10 | 15,630.7 | 5,720.3 | 16,344.2 | 5,696.6 | 16,314.9 |
| main TS-CS01 | 50 | 27,218.6 | 5,741.2 | 28,239.2 | 5,661.9 | 28,027.6 |
| main TS-LK01 | 1 | 3,657.0 | 4,978.9 | 3,828.5 | 4,939.9 | 3,806.3 |
| main TS-LK01 | 10 | 15,179.9 | 9,698.7 | 15,738.9 | 9,503.9 | 15,725.0 |
| main TS-LK01 | 50 | 25,044.4 | 9,609.5 | 24,585.0 | 9,743.6 | 25,379.4 |
| main TS-VC01 | 1 | 3,800.3 | 2,459.1 | 3,296.8 | 2,438.6 | 3,781.9 |
| main TS-VC01 | 10 | 15,744.3 | 4,049.5 | 15,945.7 | 3,999.8 | 16,206.7 |
| main TS-VC01 | 50 | 22,804.9 | 4,056.4 | 27,304.2 | 4,000.2 | 27,833.3 |
| main TS-EX01 | 1 | 1,570.8 | 717.2 | 1,567.1 | 710.9 | 1,569.9 |
| main TS-EX01 | 10 | 6,483.4 | 877.1 | 6,430.8 | 886.3 | 6,499.5 |
| main TS-EX01 | 50 | 6,275.7 | 885.3 | 6,346.9 | 884.1 | 6,405.5 |
| main TS-SS01 | 1 | 3,289.0 | 5,794.2 | 3,727.6 | 5,766.5 | 3,729.2 |
| main TS-SS01 | 10 | 15,186.0 | 11,136.4 | 15,264.5 | 10,961.8 | 14,895.6 |
| main TS-SS01 | 50 | 25,843.0 | 11,183.7 | 25,947.2 | 10,977.3 | 25,916.3 |
| main TS-CM01 | 1 | 3,699.6 | 4,684.3 | 3,732.1 | 4,712.5 | 3,709.8 |
| main TS-CM01 | 10 | 15,542.3 | 8,489.7 | 15,446.0 | 8,391.6 | 15,240.0 |
| main TS-CM01 | 50 | 24,875.2 | 8,431.9 | 24,920.7 | 8,375.2 | 24,892.3 |
| repeat TS-LK01 | 50 | 24,941.2 | 9,570.1 | 25,651.9 | 9,841.6 | 25,400.0 |
| repeat TS-SS01 | 50 | 25,640.0 | 11,158.3 | 25,901.1 | 11,294.1 | 25,821.9 |

The intermediate lookup/50-VU result regresses against baseline while its loaded membership improves; intermediate membership at 1 VU regresses. Preserve these mixed outcomes. The final cold-preparation change primarily targets unique context work; it must not be described as an across-the-board warm-read improvement.

## Process observations

Values cover each complete process group, including readiness/preflight/warmup. CPU is the mean sampled `ps` CPU percentage (100% is one core); RSS is the sampled maximum. Neither is an exclusive load-window measurement or a whole-deployment resource total. PostgreSQL, Docker VM, clients and other services are excluded.

| Cohort | Group | Process | Mean sampled CPU % | Sampled maximum RSS MiB |
| --- | --- | --- | ---: | ---: |
| baseline200 | main | term-squid | 68.1 | 91.2 |
| baseline200 | main | fhirsmith | 92.7 | 654.0 |
| baseline200 | repeat | term-squid | 85.3 | 40.6 |
| baseline200 | repeat | fhirsmith | 98.5 | 451.6 |
| candidate205 | main | term-squid | 68.5 | 89.6 |
| candidate205 | main | fhirsmith | 92.9 | 657.3 |
| candidate205 | repeat | term-squid | 84.1 | 41.1 |
| candidate205 | repeat | fhirsmith | 99.6 | 485.3 |
| final206 | main | term-squid | 68.5 | 79.5 |
| final206 | main | fhirsmith | 92.6 | 625.4 |
| final206 | repeat | term-squid | 84.7 | 40.6 |
| final206 | repeat | fhirsmith | 98.8 | 486.1 |

## Functional and cold-work evidence

The final source passes 206 Rust/PostgreSQL tests, strict workspace Clippy, formatting, the locked standalone core consumer/dependency guard and 25 production embedded browser/API scenarios. Original empty-database setup, preloaded setup, cache parity, real JSON/package activation/restart recovery and the seven unchanged local gates pass. See [the implementation report](REQUEST_CONTEXT_REPORT.md).

The [independent official audit](ECOSYSTEM_GAP_AUDIT.md) remains 110/689 versus 682/689, with all failures retained. The selected API/UCUM checks pass 49/49 versus 39/49. An additional [generated finite model](FINITE_DIFFERENTIAL_REPORT.md) passes 208/208 versus 104/208; those profiles are not interchangeable conformance scores.

A separate [mixed-context report](CONTEXT_MIXED_REPORT.md) records fixed-arrival lookup and unique cold expansions, with profiler runs excluded. The local microbenchmark reduces exact generated decode/prepare median cost from 4.190 to 2.271 ms through borrowed Serde decoding and integer graph bookkeeping. It does not establish an HTTP speedup by itself. Generation observations remain fresh on every retained read; no TTL, stale-error fallback, increased pool, weakened assertion or reduced response contract is introduced.

## Reproduction

Load identical package inputs and configure actual UCUM 2.2 on the pinned reference. Build the frontend before Cargo/RustEmbed, preserve the baseline artifact and freeze compilation inputs. Start each participant fresh and sequentially with the recorded resource limits. Run:

```sh
bun benchmarks/bench.ts run --name term-squid --url http://127.0.0.1:18081/r4 \
  --server-revision SOURCE_AND_ARTIFACT_SHA \
  --tests TS-CS01,TS-LK01,TS-VC01,TS-EX01,TS-SS01,TS-CM01 \
  --vus 1,10,50 --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 + UCUM#2.2' \
  --environment-label 'matched host, VM, pool and process configuration'
```

Repeat for FHIRsmith with its exact source revision and loopback URL. Restart both in reverse order for `--tests TS-LK01,TS-SS01 --vus 50`. Wrap with the existing process observer when collecting CPU/RSS. Do not run builds, profiles, browser tests or corpus imports during windows.

Raw manifests/results remain under `.benchmarks/results/context{200-baseline,205-candidate,206-final}-{main,repeat}/`. Exact orchestration, source audits, environment, process time series and admitted summaries are in `.benchmarks/overnight-20261105/{baseline200,candidate205,final206}-performance-*.json`. They are gitignored and must be recreated on another host; historical figures from a different machine cannot replace a new matched cohort. Full clinical corpus acquisition, native accessory/provider coverage, Termbox provisioning and the unchanged twenty-scenario composite remain separate work.
