# Prepared ValueSet increment — November 2, 2026

## Scope

This increment adds reusable immutable finite ValueSet plans to `term-squid-core` and a bounded PostgreSQL adapter cache. The first increment's [report](PERFORMANCE_REPORT.md) and raw artifacts are preserved. All measurements here use local synthetic data; they establish neither clinical conformance nor the overall 3× reference or >12,000 simple-read RPS targets.

## Execution and consistency

`PreparedValueSet` materializes the existing compose evaluator once and owns its immutable members and a system/code membership index. Membership resolves an optional business version and case policy through that index. There is no per-code answer cache: changing, missing, excluded and outside codes all use the same plan. Explicit unsupported-operation errors, imported intersections, include/exclude, typed filters and canonical resolution still pass through the existing evaluator. Cold preparation still enumerates finite systems; this is a materialized finite plan, not a symbolic filter compiler. Different root plans do not yet share prepared CodeSystems.

The adapter returns shared `Arc` handles to the plan and base resource. Expansion serializes only the selected page, scanning members for total/filter semantics without constructing a second full result vector. Each prepared plan represents one repeatable-read dependency snapshot. The transient raw/typed dependency snapshot is discarded after materialization.

A singleton PostgreSQL generation row is incremented by statement triggers on CodeSystem and ValueSet INSERT, UPDATE, DELETE and TRUNCATE. The generation update commits or rolls back with resource changes, including writes from separate CLI processes. Unlike a sequence token, its row lock serializes writers and prevents out-of-order commit tokens. Readers check the generation in their repeatable-read transaction before reusing a plan. Root, imported resources, default-version selection and explicit systemVersion context therefore come from one generation. An already started operation can finish using its old immutable plan; a request whose snapshot starts after a committed update observes the new generation. There is no notification/polling staleness interval. Missing resources and preparation errors are not cached.

Keys contain parsed root canonical/version, optional instance UUID and optional system/version context. Any relevant table change clears all cached plans on the next read, including unrelated changes; this deliberately favors correctness over selective invalidation. The cache admits at most eight entries and 64 MiB of conservative memory charges (tree/index/member buffers and base-resource JSON). Oversized plans execute without retention. Existing 32 MiB input, 128-resource, 100,000-concept and depth bounds remain. This is a retained-cache budget, not a hard process RSS or request-queue bound: old Arc handles may survive eviction, and cold preparation needs transient memory.

One async mutex serializes cache checks and cold preparation. Ownership moves into blocking preparation so a cancelled waiter cannot release admission while preparation still runs. Concurrent successful requests for the same key share one plan. This simple policy also serializes unrelated keys and warm generation queries; finer admission is future work. No raw data or answer cache grows with the number of requested codes.

The new migration must be applied before starting the new server. Application startup still does not run migrations automatically. Database owners must preserve the generation triggers and write resource content atomically; disabling triggers bypasses this protocol. The existing CLI import transactions already meet that contract. Legacy projection-update limitations from the first report remain.

## Profiling evidence

Artifacts are under `.benchmarks/profiles/increment2-*`; instrumented copies/binaries and the standalone phase probe remain ignored. The early `results/increment2-before` matrix overlapped diagnostic sampling/builds and is excluded from final throughput comparisons.

The baseline CPU sample shows SQLx JSON decoding, serde map construction, allocation/free, typed system preparation and compose selection. A separate release library probe runs 100 identical preparations of the 10,001-concept system: JSON Value decoding averages 4.985 ms, conversion plus `PreparedSystem::prepare` 11.816 ms, and compose enumeration 2.105 ms. Prepared indexed membership averages 408 ns over 100,000 changing codes, including query-code formatting. These are isolated library timings, not HTTP latency or additive end-to-end SQL costs.

A representative baseline `EXPLAIN (ANALYZE, BUFFERS)` for fetching content and its text size reports 9.477 ms execution, 0.308 ms planning and 51 shared buffer hits. It includes JSON text sizing, unlike a metadata-only index lookup. It does not include wire transfer and client JSON decoding, and one diagnostic plan is not a SQL latency distribution.

The identical isolated allocation probe performs 20 warmup and 1,000 changing-code membership requests, including 20% missing codes, asserting every response:

| Per request | Before | Prepared |
|---|---:|---:|
| Allocation/reallocation calls | 340,212.639 | 116.455 |
| Requested allocation bytes | 49,581,647.185 | 350,054.620 |

These process-wide counters include HTTP/runtime/SQL and probe overhead. Requested bytes measure churn, not retained memory. Instrumented timing is excluded from throughput. A stale shared Cargo artifact was detected during the first after-probe build; no result was used from that failed build. The helper now touches isolated Rust source files before building; the final repeated result uses `allocation-source-increment2-final`.

## Validation

All 21 Rust tests (including both opt-in database integrations), seven benchmark harness tests, formatting, workspace Clippy with warnings denied pass. Integration tests use a disposable schema and independent SQL sessions: concurrent requests share Arc identity; commit replaces positive and negative membership; uncommitted writes and rollback preserve the previous plan; imported changes, deletion, new default versions and explicit version contexts resolve correctly; eviction preserves in-flight plans. Core tests cover indexed version/case behavior and plan survival after dropping its construction snapshot. Existing semantic assertions are retained.

## Matched performance measurements

Final measurements use the same baseline binary from the first increment and the new release binary, PostgreSQL 17, default pool five, loopback port 18081, the retained synthetic package plus R4 core 4.0.1, one machine, 5-second warmup, 30-second sustained tests and 1/10/50 VU. TS-VC01 now changes code across the 10,000-code system and injects distinct missing codes on 20% of requests; both binaries use this same stronger harness. TS-EX01 retains its pagination assertions. All builds and intrusive profiles finish before the final paired timing runs. CPU/RSS observation uses the same one-second `ps` sampler on both sides.

| Operation | VU | Before RPS | Prepared RPS | Ratio | Before p50/p95/p99 ms | Prepared p50/p95/p99 ms |
|---|---:|---:|---:|---:|---|---|
| TS-VC01 | 1 | 27.7 | 1181.3 | 42.60× | 35.305/39.189/44.762 | 0.736/1.036/1.505 |
| TS-VC01 | 10 | 41.4 | 1047.8 | 25.29× | 216.556/328.130/472.609 | 8.554/14.513/18.839 |
| TS-VC01 | 50 | 44.6 | 1126.7 | 25.24× | 1025.574/1871.695/2646.171 | 39.636/63.626/81.827 |
| TS-EX01 | 1 | 27.7 | 529.8 | 19.11× | 34.472/41.634/49.749 | 1.220/2.638/4.533 |
| TS-EX01 | 10 | 50.5 | 657.1 | 13.01× | 194.248/218.948/264.326 | 12.858/26.327/44.975 |
| TS-EX01 | 50 | 48.1 | 845.5 | 17.59× | 1027.318/1147.864/1345.622 | 56.248/76.091/86.351 |

All six before and all six after measurements pass, with zero HTTP error rate and zero failed semantic checks. All seven final preflights pass. These results show a substantial improvement on this local compose workload; they do not establish the overall product targets.

| Operation | VU | Before CPU % | Prepared CPU % | Before peak RSS MiB | Prepared peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| TS-EX01 | 1 | 54.7 | 19.0 | 167.1 | 22.4 |
| TS-EX01 | 10 | 113.8 | 26.8 | 168.4 | 15.5 |
| TS-EX01 | 50 | 111.9 | 28.4 | 168.8 | 21.7 |
| TS-VC01 | 1 | 56.0 | 17.0 | 142.2 | 42.3 |
| TS-VC01 | 10 | 111.7 | 22.6 | 141.0 | 42.4 |
| TS-VC01 | 50 | 105.6 | 22.0 | 166.5 | 49.3 |

CPU is the interpolated process CPU-time delta over each measurement (100% is one core), not whole-machine or PostgreSQL CPU. RSS is the maximum observed one-second sample, not an allocator-retained-byte count. The after process was already warmed by a separate diagnostic run; neither side measures cold-start RSS. CPU and RSS fall substantially, while warm concurrency scales poorly with the current serialized transaction checks.

## Remaining limitations and next bottleneck

The prepared path still starts a transaction, sets repeatable-read isolation, queries the generation row and commits on every request. Warm checks are serialized by the cache mutex. This bounds duplicate preparation and guarantees visibility, but limits concurrency and keeps SQL round trips on the hot path. It is the next execution-design candidate; the throughput matrix alone cannot isolate lock wait from database/network scheduling. The after CPU sample contains generation-query/transaction scheduling and indexed membership; repeated full-system JSON decode and compose selection are absent from that warm sample. A representative generation-only SQL plan executes in 0.015 ms (two shared buffer hits), which excludes transaction round trips and queueing. A shared generation-read/admission design must retain the same post-commit freshness contract.

Unrelated imports invalidate every plan; many changing roots can thrash the eight-entry cache. Cold requests still transfer and decode complete resources and enumerate filters. Symbolic plans, shared prepared-system snapshots, dependency-specific generations and property/bitmap indexes need larger datasets and cold/update measurements. Process memory, queued requests, cancellation of expensive SQL work and overload policy need explicit operational budgets. No public reference was load-tested and no new FHIRsmith ratio is inferred from unmatched runs.

## Reproduction and provenance

Apply `just bench-db` and `just bench-migrate` against the retained local volume, build the selected source with `CARGO_TARGET_DIR=.benchmarks/target cargo build --offline --release --bin backend`, and run `just bench-server`. The final paired runner command is:

```sh
python3 benchmarks/observe.py --pid SERVER_PID --output .benchmarks/UNIQUE-process.json -- \
  bun benchmarks/bench.ts run --tests TS-VC01,TS-EX01 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1'
```

Use the same current harness for both server builds. Requests are iteration-indexed, so absolute positive/negative counts depend on completed iterations; semantic assertions apply on every response. Exact results, checks, error rates and manifests live in `results/increment2-matched-{before,after}/term-squid`, all seven preflights in `results/increment2-final-preflight`, CPU/RSS series and profiles in `profiles/increment2-*`, and check/build logs in `.benchmarks/increment2-*`. The baseline is the retained first-increment executable started before replacing the release path, with Git HEAD `3b5bb4ff67f038320d8bed551eeaf73239501169`; the run manifests leave serverRevision unset rather than inventing a new revision for dirty source. The final source archive is `profiles/increment2-final-source.tar.gz` (SHA-256 `19beae1b54d72d73d6f5ace0dee3ba1974eb78ba270bdf58ff042d852885290e`), and final executable SHA-256 is `c017cc81ab1248f2687d7dea2a63aca4bf871d060b071fb65801120ea928d4c6`. Baseline and after runs are sequential, not randomized repetitions; laptop scheduling/thermal effects remain a limitation.
