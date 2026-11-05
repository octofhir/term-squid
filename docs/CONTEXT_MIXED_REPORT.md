# Context read isolation

November 5, 2026. Generated local workloads compare pinned FHIRsmith with source-frozen term-squid artifacts. This is a fixed-arrival cold-work diagnostic, separate from the original benchmark dashboard/scorer and the clinical composite.

## Protocol

Each 30-second window sends exactly 3,000 ordinary lookups (100/s) using the unchanged TS-LK01 response predicate. Context expansion runs at 0, 8 or 24 requests/s; every context supplies a unique 3,000-concept, 30-level CodeSystem and requests one flat result. The selected lookup code trace is warmed at concurrency 20 before each window. Global per-scenario iteration numbers give both participants the same request identities independently of worker scheduling. Each response is checked; admission requires exact scheduled counts, zero dropped arrivals, zero HTTP/semantic failures and complete latency data. k6 boundary callbacks produce no additional HTTP request and remain visible in raw metrics.

Both participants use the same synthetic package, complete R4 core 4.0.1 and UCUM 2.2 on the same nondedicated Apple M1/16-GiB host. PostgreSQL 17 runs in the existing 8-CPU/8,393,605,120-byte Docker VM; term-squid uses pool 32. FHIRsmith is pinned to `25a41f87711507789b361480ee4efae42df504d4`, with the documented bootstrap-only R4 core pin. Source audit, exact fixture/harness hashes, process observations and response metrics are retained locally. No clinical releases, composite score or production capacity guarantee are asserted.

Fresh server processes run sequentially: reference first for the three primary levels, then candidate first for the 8/s restart repeat. Builds, automated browsers and CPU profiling are excluded from timing windows. Earlier incomplete generator experiments are retained as unscored diagnostics.

## Before prepared-source reuse

The 194-test artifact is `4d933bf994df3ddca139c1f1d07a494dddcbed68c95df0516210afcf1f8ed6af`. Eight valid windows pass **26,400 response checks**, with no dropped arrivals or errors. Raw summary: `.benchmarks/overnight-20261105/mixed-v2-current-summary.json`; all windows share harness SHA-256 `156dd1cd593e52b5e4cf4f5506d45e17418148130e6e8c6f626d4bd065ccb61e`.

| Group | Server | Context requests/s | Lookup p99 ms | Context p99 ms |
| --- | --- | ---: | ---: | ---: |
| main | fhirsmith | 0 | 3.170 | — |
| main | fhirsmith | 8 | 30.804 | 37.485 |
| main | fhirsmith | 24 | 16.204 | 17.348 |
| main | term-squid | 0 | 2.858 | — |
| main | term-squid | 8 | 2.456 | 50.439 |
| main | term-squid | 24 | 1.852 | 22.706 |
| repeat | term-squid | 8 | 1.974 | 45.945 |
| repeat | fhirsmith | 8 | 32.165 | 42.248 |

Ordinary lookup remains responsive in this measured cohort while cold context expansion is slower than the reference. The 8/s restart repeat preserves both observations. The two participants also change latency across phases; retain every window rather than attributing the difference to one assumed cause.

## Profile and implementation

A separate `sample` CPU profile of the same generated workload identifies repeated `PreparedSystem::prepare_inner`/graph validation beneath `compose_loader::prepare_rich`. It is a profiling-only run and is excluded from timing comparisons. The subsequent implementation transfers context JSON ownership and reuses request-owned immutable prepared systems through additive core APIs. Strict lifecycle/content checks, bounded evaluation and compact code knowledge remain enforced. The existing owned engine API stays available; completed plans do not retain the shared input view. See [the implementation report](REQUEST_CONTEXT_REPORT.md).

## After prepared-source reuse

The 200-test artifact is `50987ff00778d3a232be3a757a97f572ef1b9cc18420ebbc94c2bbdd0ebc7e9c`. The identical eight-window protocol again passes **26,400 response checks**, with no dropped arrivals or errors. Raw summary: `.benchmarks/overnight-20261105/mixed-v3-current-summary.json`.

| Group | Server | Context requests/s | Lookup p99 ms | Context p99 ms |
| --- | --- | ---: | ---: | ---: |
| main | fhirsmith | 0 | 2.742 | — |
| main | fhirsmith | 8 | 29.782 | 37.282 |
| main | fhirsmith | 24 | 16.410 | 16.948 |
| main | term-squid | 0 | 3.261 | — |
| main | term-squid | 8 | 6.125 | 46.492 |
| main | term-squid | 24 | 1.519 | 20.158 |
| repeat | term-squid | 8 | 5.871 | 46.642 |
| repeat | fhirsmith | 8 | 31.639 | 38.048 |

The candidate preserves the ordinary-read advantage over the matched reference during context work. Context p99 remains slower than the reference in every loaded window. Candidate-versus-baseline lookup p99 regresses at 8/s and in the restart repeat, while the 24/s context p99 falls from 22.706 to 20.158 ms. These separate nondedicated-host cohorts do not establish an across-the-board performance improvement. The reuse change removes duplicated preparation and its bounded ownership tests pass; retain the latency regressions and all raw windows.

Both cohorts share fixture SHA-256 `310192a633ed73574ffd39b8b82720f948ac817716113e69a6003bb07366e94d` and the harness hash recorded above. After all 52,800 checks, the matched database still contains 1,064 CodeSystems, 1,317 ValueSets and 81 ConceptMaps with unchanged content fingerprints. These are actual package/synthetic resource counts, not clinical coverage.

## Final graph and JSON preparation comparison

The 205-test checkpoint (`06c0cc8bd50d554da56ae6ebc3f915b1e37224dcd4b292cba7fdd03e65a14669`) and final 206-test checkpoint (`6ddb9ad1d0ccb5a62c6a56cbf28ec7d7bc567ddffdbd03dbb1f11d1510cb0ae0`) each repeat all eight windows. Each passes **26,400 response checks**, with zero errors or dropped arrivals. The fixture, arrival trace, warmup and harness hashes remain identical to the earlier cohorts. Raw summaries are `mixed-v4-current-summary.json` and `mixed-v5-current-summary.json`.

| Checkpoint | Group | Server | Context requests/s | Lookup p99 ms | Context p99 ms |
| --- | --- | --- | ---: | ---: | ---: |
| 205 | main | fhirsmith | 0 | 3.094 | — |
| 205 | main | fhirsmith | 8 | 31.716 | 37.235 |
| 205 | main | fhirsmith | 24 | 16.495 | 16.588 |
| 205 | main | term-squid | 0 | 2.781 | — |
| 205 | main | term-squid | 8 | 6.784 | 48.217 |
| 205 | main | term-squid | 24 | 1.680 | 20.105 |
| 205 | repeat | term-squid | 8 | 6.424 | 48.388 |
| 205 | repeat | fhirsmith | 8 | 32.812 | 37.240 |
| 206 final | main | fhirsmith | 0 | 2.973 | — |
| 206 final | main | fhirsmith | 8 | 32.351 | 39.310 |
| 206 final | main | fhirsmith | 24 | 16.272 | 17.029 |
| 206 final | main | term-squid | 0 | 2.734 | — |
| 206 final | main | term-squid | 8 | 7.132 | 45.498 |
| 206 final | main | term-squid | 24 | 2.195 | 20.881 |
| 206 final | repeat | term-squid | 8 | 8.235 | 45.857 |
| 206 final | repeat | fhirsmith | 8 | 31.944 | 37.814 |

The final candidate preserves substantially lower ordinary lookup tails than the matched reference while context work runs. It does not improve every metric against its own baseline: 8/s context p99 improves, 24/s context p99 regresses, and loaded lookup tails regress against checkpoint 205. Context expansion remains slower than FHIRsmith. The isolated decoder/graph cost reduction therefore cannot be equated to the same HTTP speedup or an across-the-board isolation improvement.

A separate 205-artifact CPU sample confirms string comparisons in graph admission. The final implementation assigns local integer identities for queue/depth updates and deserializes owned CodeSystem definitions directly from a retained JSON reference. The independent 160-graph shortest-path model and complete 206-test workspace pass without relaxing cycle, depth, traversal, source or cancellation limits. The isolated 3,000-concept decode/prepare median falls from 4.190 to 2.271 ms; this profiler/microbenchmark work is outside every compared window.

Across all four admitted mixed cohorts, **105,600 response checks** pass. The 120 warm-suite windows and their 37,208,534 checks are reported separately in [REQUEST_CONTEXT_PERFORMANCE.md](REQUEST_CONTEXT_PERFORMANCE.md). Neither workload establishes clinical coverage, a weighted composite, a deployment p99 guarantee or the overall 3x product goal.
