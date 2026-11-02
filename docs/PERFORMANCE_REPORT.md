# Correctness and Performance Increment — November 2, 2026

## Scope and outcome

This increment removes the measured full-CodeSystem read from lookup/validation, implements a shared bounded compose evaluator, fixes the seven local semantic scenarios, and starts a reusable `term-squid-core` library. It preserves the original benchmark assertions and existing benchmark/documentation work. No commit, push or deployment was performed.

All seven synthetic preflights pass: CodeSystem validation, versioned lookup, ValueSet membership, paginated expansion, hierarchy subsumption, forward translation and reverse translation. This is a local regression result, not full FHIR R4 conformance. The user target of more than 12,000 RPS for simple reads and the proposed 3× reference throughput target remain goals, not established product results.

## Measured bottleneck

Before the change, validation and lookup performed `SELECT * FROM code_systems`, decoded its complete JSON into `serde_json::Value`, and then fetched one concept in a separate SQL round trip. The synthetic CodeSystem has 10,001 concepts: its JSON text is 1,119,316 bytes, versus 88,931 bytes stored compressed by PostgreSQL. Every request repeated that transfer, allocation and decode, including requests for missing codes.

The baseline CPU sample contains SQLx JSON decoding, serde map/array construction and allocator/free stacks. `EXPLAIN (ANALYZE, BUFFERS)` reports approximately 0.066 ms for the indexed row lookup; this does **not** account for client-side JSON decoding or the complete cost of transmitting a TOASTed value. The compact resolver's representative plan is also indexed (approximately 0.089 ms, including its concept lookup). The improvement comes from eliminating unnecessary data and the extra round trip, rather than claiming that the index scan itself became faster.

A separate instrumented build wraps Rust's system allocator. Its Bun fetch driver uses the same URLs and client negotiation on both sides; it is separate from the k6 timing client. The identical workload performs 20 warmup requests and then 1,000 changing-code validation requests, including 20% missing codes, with semantic assertions on every response:

| Allocation metric per request | Before | After |
|---|---:|---:|
| alloc / alloc_zeroed / realloc calls | 120,163.861 | 101.665 |
| Requested allocation bytes | 18,954,892.145 | 344,882.111 |

These are process-wide allocation deltas, including HTTP, runtime, SQLx and probe overhead. Requested bytes measure allocation churn, not retained memory or allocator size classes. Instrumentation substantially changes timing; its elapsed time is excluded from all throughput claims. The counts fall by about 1,182× and requested bytes by about 55×. Small concept-property JSON is still decoded; full terminology JSON is absent from this hot path.

CPU samples and `heap` snapshots were taken separately from final throughput measurements. `heap` is intrusive; its snapshots are diagnostic artifacts and are not a matched before/after RSS comparison. An initial final CPU sample missed the active load; use `after/cpu-validate-active.txt`, not the idle sample.

## Implementation

- A single indexed, bounded SQL row now resolves terminology metadata and the requested concept. Stored generated columns expose completeness and case-sensitivity without detoasting resource JSON. They update atomically with content changes, including CLI imports. Lookup returns the selected business version. Missing terminology/version is distinct from a missing code in a complete system. Display mismatches produce `result=false`.
- `term-squid-core` has no Axum, Tokio or SQLx dependency. It owns canonical/business-version parsing, typed concepts/properties, immutable prepared systems, hierarchy validation and compose set operations. CLI package import and single-resource creation share its recursive concept preparation. Import writes concepts in batches and materializes ancestry, inside the same transaction as the resource.
- Membership and expansion use the same evaluator: union across includes, intersection of imported ValueSets within one selection, system/import intersections, subtraction of excludes, deduplication, whole systems, explicit concepts, inactive selection, versioned references, and supported property/hierarchy filters. Supported operators are typed code/string/boolean/integer equality, property exists, and concept `is-a`, `descendent-of`, `is-not-a`. Unsupported operators, Coding/decimal/dateTime equality, unknown properties/codes, non-complete enumerations and locked dates fail explicitly. Compound or decimal values are never compared by serializing them to JSON strings. Generic composition supports explicit boolean inactive/notSelectable flags, but refuses lifecycle status/date properties and unsupported standard-URI aliases until provider-specific rules exist; it does not silently classify retired concepts as active. Property meaning is identified by URI in [FHIR R4 CodeSystem](https://hl7.org/fhir/R4/codesystem.html#properties). See the [FHIR R4 compose definition](https://hl7.org/fhir/R4/valueset-definitions.html#ValueSet.compose.include) for the import/intersection contract.
- The SQL adapter resolves the root resource and all dependencies in one repeatable-read transaction. Its immutable operation snapshot survives after the transaction closes. There is no process-wide content/answer cache to become stale after CLI import. `systemVersion` selects an otherwise unversioned system during membership evaluation; an explicitly constrained compose version remains authoritative.
- Limits are 32 nested levels, 128 resolved resources, 32 MiB of input JSON, 100,000 prepared concepts per operation, 100,000 output members, a maximum page size of 10,000, and two simultaneous compose preparations. CPU preparation/evaluation runs on the blocking pool; the semaphore permit stays with the worker even if its caller is cancelled. Total request queue length, global scheduling and cancellation budgets still need production limits.
- Expansion no longer reads a nonexistent column or silently returns an empty set. The legacy materialized-expansion query now uses `expansion_data`; compose evaluation does not trust an unversioned persisted expansion cache. POST integer pagination works, count=0 returns an empty page with the full total, negative pagination is rejected, and only the selected page is serialized to FHIR contains entries.
- Hierarchy migration backfills nested concepts and ancestry for existing imports. New imports validate cycles/dangling complete-system parents and preserve transitive distances. Subsumption respects the selected resource row, including versionless instances. Reverse translation matches target codes, returns source codes/displays, inverts directional equivalence, and applies `targetsystem`; disjoint/unmatched mappings are not positive matches. Conditional mappings and target-ValueSet constraints fail explicitly.
- Resource reads return stored FHIR JSON. Application errors return OperationOutcome and avoid exposing database/internal diagnostics. CapabilityStatement no longer advertises nonexistent REST create/update/delete or version search, and identifies R5/R6 paths as unverified aliases.

## Validation

- `cargo fmt --all --check` and `git diff --check` pass.
- `cargo clippy --offline --workspace --all-targets -- -D warnings` passes. A narrowly scoped allowance handles the `async-trait` macro's redundant must-use attribute under Clippy 1.99; application lints remain enabled.
- `cargo test --offline --workspace -- --include-ignored` with `TERM_SQUID_TEST_DATABASE_URL` pointing to the isolated local benchmark database passes 19 tests: 14 library cases, 2 mapping tests, 1 configuration-boundary test, 1 HTTP/SQL integration test and 1 CLI import test. Database tests create and remove unique temporary schemas.
- Regression cases include positive/negative membership, duplicate includes, imported intersections, exclusions, inactive selection, hierarchy/property filters, unsupported filters, cycles/depth, version conflicts, case-insensitive explicit codes, typed equality and explicit unsupported-value errors, exact instance context, display mismatch, POST pagination, unavailable versions, content refresh, recursive import, transitive depth and rollback on invalid hierarchy.
- `bun test benchmarks` passes 7 harness tests / 26 assertions. The original local semantic assertions remain unchanged.

## Measurement protocol

Apple M1 (8 logical CPUs, 16 GiB RAM), macOS arm64, Rust 1.99.0, Bun 1.4.3, k6 2.3.0, PostgreSQL 17.11 (aarch64) in the existing local Docker volume; Docker reports 8 CPUs and 8,393,605,120 bytes of VM memory. The primary release matrix uses the original five-connection SQLx pool. A later focused experiment varies pool capacity on the same configurable build; it is reported separately. No dedicated CPU/RAM limits or exclusive machine reservation were applied; the database, client and server share the laptop. Measurements are descriptive single-machine runs, not confidence intervals or production sizing.

Both sides use the same synthetic fixture version 1.0.0, fingerprint `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`, plus R4 core 4.0.1. Fixture semantics were not changed. The hierarchy migration adds the intended index data for those same resources. Each scenario warms up for 5 seconds at 1 VU, then runs for 30 seconds at each of 1/10/50 VU. k6 uses closed concurrency and checks every response. Lookup cycles through 10,000 codes; validation uses 8,000 positive codes from that pool plus 20% missing-code requests. Membership rotates the included/excluded/outside probes 7/42/999, expansion rotates three offsets, and translations cycle through 128 mappings. These small compose/mapping pools are not a substitute for clinical or large-plan workloads. p50/p95/p99 are client-observed HTTP durations in milliseconds. Profilers, allocator counters and builds are excluded from the final speed runs.

The baseline source is commit `2c6a483e4c6946dc5c8c97f05d4e857ced1b26b9`. The after builds are uncommitted working-tree stages of this increment. Binary SHA-256 values:

- Before: `9798bac21070cf0eac0c8f58f1a0ff6c6273735f3903a0467e184c33dd1c78c8`.
- Primary after matrix / paired checks (build A): `9599ddc74aab55327250d76c88423a8bec6cd548269d20eaa5273c09e620daa5`.
- Configurable-pool experiment build B: `fc99c6536069d8428cb1196f336bfcb41d8a6d8997bd6c4a22bd0cb20c9bd86e`. Its only subsequent production-code change adds bounded startup configuration for pool capacity (default remains 5); terminology request logic is unchanged.
- Final correctness-guard build C: `dad6d5681d90690233b6cce4c1c47526d119748d55853dcfa1657a6b21b3c2be`. It additionally rejects unsupported Coding/decimal/dateTime equality instead of comparing JSON strings, with a dedicated regression test. Benchmark fixtures use no such filters. It also explicitly refuses unsupported standard property aliases/lifecycle policies. Final all-seven preflight passes on build C; its affected compose workloads are remeasured separately below. Earlier timed matrices remain explicitly attached to A/B rather than relabelled as C.

## Throughput and latency

The primary after run (build A) passes **7/7 preflights, 21/21 measurements and 1,304,312 semantic checks**, with zero HTTP errors and zero failed load assertions. Compose was invalid before this change, so it receives no baseline speed score.

Primary before/after comparison uses the later clean baseline, without builds or profilers running:

| Test | VU | Before req/s | After req/s | After / before | Before p99 ms | After p99 ms |
|---|---:|---:|---:|---:|---:|---:|
| TS-CM01 | 1 | 1,476.2 | 1,494.8 | 1.01× | 2.018 | 1.091 |
| TS-CM01 | 10 | 3,945.7 | 3,633.6 | 0.92× | 4.331 | 4.292 |
| TS-CM01 | 50 | 4,059.3 | 3,471.0 | 0.86× | 17.905 | 23.117 |
| TS-CS01 | 1 | 82.1 | 2,026.0 | 24.67× | 13.689 | 0.831 |
| TS-CS01 | 10 | 196.3 | 4,448.0 | 22.65× | 93.214 | 6.078 |
| TS-CS01 | 50 | 239.5 | 4,472.2 | 18.68× | 292.678 | 27.741 |

Complete after matrix (all rows have zero errors and failed semantic checks):

| Test | VU | req/s | p50 ms | p95 ms | p99 ms |
|---|---:|---:|---:|---:|---:|
| TS-CM01 | 1 | 1,494.8 | 0.564 | 0.770 | 1.091 |
| TS-CM01 | 10 | 3,633.6 | 2.560 | 3.551 | 4.292 |
| TS-CM01 | 50 | 3,471.0 | 13.606 | 17.895 | 23.117 |
| TS-CM02 | 1 | 1,472.3 | 0.572 | 0.797 | 1.186 |
| TS-CM02 | 10 | 3,533.3 | 2.624 | 3.684 | 4.488 |
| TS-CM02 | 50 | 3,526.0 | 13.488 | 17.347 | 20.945 |
| TS-CS01 | 1 | 2,026.0 | 0.405 | 0.585 | 0.831 |
| TS-CS01 | 10 | 4,448.0 | 2.022 | 2.792 | 6.078 |
| TS-CS01 | 50 | 4,472.2 | 10.450 | 13.621 | 27.741 |
| TS-EX01 | 1 | 28.2 | 34.332 | 38.057 | 45.462 |
| TS-EX01 | 10 | 47.7 | 212.054 | 238.475 | 255.969 |
| TS-EX01 | 50 | 45.9 | 1,101.401 | 1,180.581 | 1,197.727 |
| TS-LK01 | 1 | 1,980.6 | 0.415 | 0.569 | 0.760 |
| TS-LK01 | 10 | 4,461.9 | 2.011 | 2.692 | 6.769 |
| TS-LK01 | 50 | 4,458.2 | 10.430 | 13.553 | 32.152 |
| TS-SS01 | 1 | 773.8 | 1.138 | 1.615 | 2.180 |
| TS-SS01 | 10 | 1,723.2 | 5.489 | 7.124 | 10.949 |
| TS-SS01 | 50 | 1,741.5 | 27.693 | 35.666 | 47.355 |
| TS-VC01 | 1 | 29.1 | 33.921 | 35.719 | 38.931 |
| TS-VC01 | 10 | 46.4 | 223.288 | 242.088 | 251.386 |
| TS-VC01 | 50 | 46.0 | 1,076.066 | 1,235.467 | 1,569.709 |

At 10 VU, validation improves 22.65× over the clean baseline, but 4,448 req/s is below the >12k target. Membership/expansion are correct but only approximately 46–48 req/s: repeated preparation is the next measured bottleneck. Forward translation did not improve: the full-matrix run is 8% lower at 10 VU and 14% lower at 50 VU than the clean baseline. Paired follow-up measurements assess this separately; no translation speedup is claimed.

Process observations at 10 VU (100% CPU = one logical CPU):

| Test | Before server CPU % | After server CPU % | Before sampled peak RSS MiB | After sampled peak RSS MiB |
|---|---:|---:|---:|---:|
| TS-CM01 | 91.9 | 88.2 | 235.8 | 24.6 |
| TS-CM02 | — | 91.0 | — | 25.6 |
| TS-CS01 | 206.0 | 51.3 | 219.9 | 39.5 |
| TS-EX01 | — | 113.3 | — | 186.8 |
| TS-LK01 | — | 51.1 | — | 21.6 |
| TS-SS01 | — | 47.2 | — | 14.3 |
| TS-VC01 | — | 108.5 | — | 126.3 |

CPU is estimated from cumulative process CPU-time deltas, interpolated at each k6 window using its end timestamp and iterations/throughput duration. RSS is sampled once per second, includes allocator/OS effects and is not a precise allocation or leak measurement. These are server-process metrics; PostgreSQL and k6 CPU/RSS are excluded. Validation at 10 VU uses approximately 10.5 ms of server CPU per request before versus 0.115 ms after. The after run's largest sampled RSS across the complete matrix is 199.3 MiB.

Primary artifacts: `results/increment-baseline-clean/term-squid`, `results/increment-final/term-squid`, `profiles/baseline/clean-process.json`, `profiles/after/final-process.json`. The initial `increment-baseline` is preserved; validation ranges from 190–239 req/s at higher concurrency across those two baselines, demonstrating local run variability. The initial baseline's first map measurement overlaps a diagnostic sample and is not the primary map comparison.


A sequential before/after/after/before follow-up repeats only TS-CM01, with 30 seconds at each concurrency and a 5-second warmup per run:

| VU | Before A req/s | After A req/s | After B req/s | Before B req/s | Mean after / mean before |
|---|---:|---:|---:|---:|---:|---:|
| 10 | 4,150.3 | 4,163.2 | 4,078.9 | 4,098.6 | 0.999× |
| 50 | 4,113.0 | 3,990.2 | 3,864.1 | 4,119.5 | 0.954× |

All eight follow-up measurements pass their checks. At 10 VU the paired result is essentially unchanged; at 50 VU the after mean is about 4.6% lower. This does not establish the initial 14% gap as a stable intrinsic regression, but it also does not prove a gain or rule out a smaller regression. Keep the original matrix visible; profile mapping allocation/decoding and mixed/order effects before claiming performance parity. Artifacts: `results/increment-pair-{before-a,after-a,after-b,before-b}` and the corresponding `profiles/pair-*-process.json` files.

## Local FHIRsmith reference

The unchanged checkout is FHIRsmith 0.18.2 / txVersion 1.10.1 at commit `25a41f87711507789b361480ee4efae42df504d4`, running Node 24.16.0. Its local configuration loads the identical synthetic resources through `folder:`; it also loads R4 core 4.0.1. npm audit and unrelated services are disabled. Default content/expansion caching is retained (30-minute cache timeout and 1,000 expansion-cache entries); no reference code or worker logic is changed. Requests use loopback port 17003 at `/tx/r4`; no public server is load-tested.

Its preflight passes six scenarios. Reverse translation reports `result=true` but returns the target coding T00007 instead of the requested reverse result C00007; the raw response is preserved. That scenario receives no speed score and its assertion is not weakened. This difference needs a specification/compatibility decision before publishing a broad compatibility claim. The reference also returns a richer response than term-squid for some operations; throughput ratios describe these measured responses and the shared assertions, not identical full conformance.

The initial reference run overlapped implementation/build work and is retained only as diagnostic evidence. The final reference run is sequential with the final term-squid run and has separate CPU/RSS observations. Historical public benchmarks on different hardware, datasets and FHIRsmith versions are not used for speedup ratios.


The final reference has **6/7 preflights and 18 passing load measurements**; reverse translation is excluded. At 10 VU:

| Test | term-squid req/s | FHIRsmith req/s | term-squid / reference | Reference p50 ms | Reference p95 ms | Reference p99 ms | Reference CPU % | Reference sampled peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| TS-CM01 | 3,633.6 | 7,457.2 | 0.487× | 1.231 | 1.922 | 2.820 | 105.0 | 597.4 |
| TS-CS01 | 4,448.0 | 5,191.9 | 0.857× | 1.816 | 2.679 | 4.692 | 105.0 | 437.2 |
| TS-EX01 | 47.7 | 871.2 | 0.055× | 11.286 | 15.717 | 17.648 | 101.7 | 595.7 |
| TS-LK01 | 4,461.9 | 8,525.3 | 0.523× | 1.012 | 1.839 | 3.601 | 103.8 | 515.0 |
| TS-SS01 | 1,723.2 | 9,876.2 | 0.174× | 0.896 | 1.535 | 3.109 | 104.4 | 597.2 |
| TS-VC01 | 46.4 | 3,933.9 | 0.012× | 2.331 | 4.443 | 6.501 | 122.9 | 548.8 |

The reference is faster on all six shared scenarios in this matrix. The proposed 3× overall target is not achieved. Lookup is approximately 1.9× slower, membership 85× slower and expansion 18× slower than this local reference at 10 VU; the latter two gaps directly motivate reusable preparation. This comparison neither scores the failed reverse response nor averages incompatible operations into a composite. All 1/10/50 VU reference p50/p95/p99, req/s, errors and check counts remain in `results/increment-fhirsmith-final/fhirsmith`; process samples are in `profiles/fhirsmith-process.json`.

## Focused database-pool experiment

A focused capacity experiment uses build B on both sides, identical requests/data, 5 seconds of warmup and 30 seconds at each of 10/50 VU. The only runtime change is `DATABASE_MAX_CONNECTIONS=5` versus `32`; no prepared-data or response cache is introduced. Both matrices pass all four measurements with zero HTTP/semantic failures.

| Test | VU | Pool 5 req/s | Pool 32 req/s | Ratio | Pool 5 p99 ms | Pool 32 p99 ms | Pool 5 server CPU % | Pool 32 server CPU % |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| TS-CS01 | 10 | 5,268.0 | 6,596.4 | 1.25× | 5.327 | 4.013 | 56.7 | 65.4 |
| TS-CS01 | 50 | 5,147.5 | 7,156.5 | 1.39× | 24.762 | 23.577 | 55.6 | 67.5 |
| TS-LK01 | 10 | 5,139.2 | 6,331.5 | 1.23× | 5.947 | 4.317 | 58.2 | 63.9 |
| TS-LK01 | 50 | 5,082.0 | 7,045.0 | 1.39× | 26.048 | 25.583 | 57.0 | 66.6 |

More connections improve these reads by 23–39% here, supporting the pool-capacity hypothesis, but the best observed 7,156 req/s still falls below 12,000. A larger pool alone does not establish the product target or the 3× reference goal. The default remains 5; the new startup setting accepts 1–64 and explicitly exposes the database-session tradeoff. Compose retains its independent two-worker bound and is not scored at the larger pool. Do not combine these focused results with the primary five-connection matrix into an overall score. Artifacts: `results/increment-pool-{5,32}/term-squid`, `profiles/pool-{5,32}-process.json` and their summaries.

## Final safety-build compose verification

The final safety build C passes all seven original preflights. Its affected membership/expansion workloads are repeated with the unchanged 1/10/50 VU, 30-second / 5-second-warmup protocol, default pool 5 and original assertions. All six measurements pass, with zero HTTP errors and zero failed semantic checks.

| Test | VU | req/s | p50 ms | p95 ms | p99 ms |
|---|---:|---:|---:|---:|---:|
| TS-EX01 | 1 | 30.8 | 31.884 | 33.539 | 34.345 |
| TS-EX01 | 10 | 51.8 | 191.700 | 199.568 | 204.734 |
| TS-EX01 | 50 | 51.4 | 966.063 | 1009.915 | 1068.129 |
| TS-VC01 | 1 | 31.3 | 31.578 | 33.197 | 34.573 |
| TS-VC01 | 10 | 52.0 | 191.625 | 201.502 | 204.935 |
| TS-VC01 | 50 | 51.9 | 961.516 | 978.113 | 982.043 |

The final build remains near 52 req/s on these operations at higher concurrency. The additional guards introduce no observed compose slowdown in this repeat; the difference from the primary 46–48 req/s also reinforces the need to treat these laptop runs as descriptive. Repeated preparation remains unresolved. Artifacts: `results/increment-final-safety-preflight`, `results/increment-final-safety-compose`, `profiles/after/final-safety-compose-process.json` and its summary.

## Remaining work and next priority

1. **Prepared generation snapshots and ValueSet plans.** Membership/expansion currently rebuild and parse complete systems per operation and enumerate the set for membership. This preserves semantics but leaves a major CPU/allocation bottleneck. Build reusable bounded snapshots/plans keyed by generation and resolved dependencies; activate updates atomically across CLI and HTTP processes. Do not add an answer cache that misses external imports. Simple reads still query PostgreSQL; milestone 2's database-free hot path is not achieved.
2. **Complete the R4 contract.** Ecosystem conformance, Coding/CodeableConcept and inline resources, language/designations, supplements, display policies, standard property URI aliases and lifecycle/default policies, complete typed property output, map source/target version context, parameters beyond the implemented subset and comprehensive capabilities remain open. Some unknown operation parameters and framework extraction errors still need explicit FHIR handling. Resource logical-id addressing and search totals/version filtering also need a separate correctness pass. Legacy backend storage create/update methods are not public REST operations and do not yet share the CLI projection rebuild pipeline.
3. **Hierarchy lifecycle.** New CLI imports are atomic and validated. The bounded migration backfill is not a replacement for validating every pre-existing hierarchy; deep/cyclic legacy data and later edits require a validated rebuild/activation workflow. Subsumption still uses multiple SQL statements rather than one immutable database snapshot for every operation.
4. **Operational limits.** Add bounded admission/queues, request/work/memory budgets, cancellation across SQL/CPU stages, readiness, metrics and explicit overload responses. Test heavy expansion mixed with simple reads. Current per-operation limits are foundations, not a full service protection model.
5. **Broader measurements.** Add larger/versioned fixtures, more negative and display cases, open-arrival-rate load, cold preparation/import/update/startup costs, UCUM/LOINC providers and ecosystem scenarios. Repeat comparative runs on a stable dedicated machine before claiming the overall 3× goal or >12k RPS. No clinical LOINC/SNOMED content is substituted with synthetic scores.

## Reproduction and local artifacts

Use `just bench-setup`, `just bench-server`, `just bench-preflight --strict`, then:

```sh
bun benchmarks/bench.ts run --vus 1,10,50 --duration 30s --warmup 5s --strict \
  --run-id YOUR_UNIQUE_RUN --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1'
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --offline --workspace -- --include-ignored
```

See [the profiling instructions](../benchmarks/README.md#separate-profiling) for allocator builds and process observation. PostgreSQL migration files live in `crates/backend/migrations`; normal application startup does not apply them automatically.

Raw artifacts and local configuration are deliberately gitignored under `.benchmarks/`: `results/increment-baseline`, `results/increment-baseline-clean`, `results/increment-final`, `results/increment-final-safety-preflight`, local-reference/pool/paired/profile-only runs, `profiles/baseline`, `profiles/after`, `fhirsmith`, instrumented source/binaries and test/build logs. Earlier `audit-*` results remain intact. Exact final run directories and process summaries accompany the tables above. The local handoff remains at `.benchmarks/PERFORMANCE_WORK.md`. Helper smoke builds pass for the current tree and the pinned historical revision; a repeated 1,000-request current probe reproduces 101.666 allocator calls / 344,882.140 requested bytes per request. The historical lockfile needed only its local package-version metadata normalized from 0.1.1 to the 0.1.2 manifests; registry dependency resolution remains locked and offline.


Final source snapshots are preserved in `profiles/after/source-primary.tar.gz` (A),
`profiles/after/source-pool-build.tar.gz` (B) and `profiles/after/source-final.tar.gz` (C).
The final source archive SHA-256 is `730be13e585b9ce60a76c4a9dd161ae341663aa2eb723fb4666a5f3f3f88c5db`.
All server processes started for these experiments and the benchmark container are stopped;
the PostgreSQL volume, imported resources and all measurement artifacts are retained.
