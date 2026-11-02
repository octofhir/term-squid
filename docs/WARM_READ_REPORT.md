# Concurrent warm ValueSet reads — November 2, 2026

## Outcome and scope

At 10 VU, the new term-squid build exceeds the pinned local FHIRsmith on the two changed workloads: membership 4,978 versus 3,319 RPS (1.50×), expansion 2,932 versus 600 RPS (4.89×). Primary 50-VU membership still loses to the reference and is unstable across repetitions. The broader goal of consistently outperforming FHIRsmith is not yet achieved.

This increment removes the measured serialization of warm ValueSet generation checks. It continues the uncommitted prepared-data work; the [first report](PERFORMANCE_REPORT.md), [prepared-data report](PREPARED_DATA_REPORT.md), source snapshots and raw results remain unchanged. No commit, push or deployment was performed. Measurements use only local synthetic terminology.

## Execution and freshness

A warmed membership or expansion request now executes one autocommit `SELECT generation` using the configured PostgreSQL pool, followed by a short cache lookup. Previously the global preparation mutex covered BEGIN, SET REPEATABLE READ, SELECT, COMMIT and the lookup. Warm database exchanges now run concurrently, and no cache lock spans SQL or preparation. There is still a database query per warm request; SQLx pool health/protocol exchanges add network traffic beyond that SELECT. This is not a database-free hot path.

The standalone SELECT is the warm operation's snapshot boundary. A cached immutable plan is eligible only when its generation exactly matches that observation. A request starting after a committed CLI update therefore cannot return a previous-generation plan. An operation already observing an older generation may finish against it; that is the existing snapshot contract. There is no TTL, delayed invalidation or stale fallback when the generation query fails.

On a miss, a separate owned async mutex admits one cold preparation. The operation rechecks the generation and cache in a new repeatable-read transaction after waiting. Dependencies, selected versions and the root resource still come from one transaction. Warm hits bypass cold admission entirely. The preparation guard moves into blocking work until that work completes, preserving single-flight behavior if the awaiting HTTP request is cancelled. Cancellation before preparation does not leave an admission entry behind. No map of pending keys grows with request diversity.

Concurrent observations can finish out of order. Cache generations only advance: an older observation never clears newer entries, and an older preparation cannot publish over a newer generation. Its initiating operation may still use its own immutable result. Missing resources and preparation failures remain uncached. The existing eight-entry/64 MiB conservative charge, transient input/resource/concept/depth limits and cross-process transactional generation triggers are unchanged. This increment does not require another database migration.

## Validation and profiling

Formatting and workspace Clippy with warnings denied pass. All 22 Rust tests pass with both PostgreSQL integrations enabled. Eight benchmark harness tests pass, including 2,000 response assertions for the membership sampler. All seven local semantic preflights pass on the final build.

New checks exercise out-of-order generation observation and late publication, warmed Arc reuse while cold admission is paused, cancellation of a waiting cold request, and four concurrent readers while another SQL session commits twelve paired ValueSet/CodeSystem updates. Readers assert that root metadata and concept display always contain the same marker. A final read after writer completion must see marker 12. Previous update, rollback, version, dependency, negative membership, deletion and eviction assertions remain intact.

CPU samples and allocation instrumentation are separate from final throughput runs. The warm path's reduction from four SQL statements to one follows directly from the adapter and the transaction paths visible in CPU samples; samples do not establish individual SQL latency distributions or distinguish every scheduler/lock wait. The library membership index and finite plan semantics are unchanged.

An identical isolated probe performs 20 warmup and 1,000 changing-code membership requests, with 20% missing codes and response assertions:

| Per request | Previous prepared path | Concurrent warm path |
|---|---:|---:|
| Allocation/reallocation calls | 116.454 | 94.448 |
| Requested allocation bytes | 350,046.417 | 345,709.721 |

These process-wide counters include runtime, HTTP, SQL and probe overhead. Requested bytes measure allocation churn, not retained memory. Instrumented elapsed times are excluded from performance scores. This change mainly removes round trips and serialization; it does not remove the large remaining requested-byte allocation floor.

## Matched performance

Both final servers start as fresh processes, using the same retained PostgreSQL 17 database, default pool five, Apple M1 host, loopback port 18081, 5-second warmup and 30-second sustained tests at 1/10/50 VU. All compilation and intrusive diagnostics finish before the final paired runs. The servers run sequentially. CPU/RSS use the same one-second `ps` observation on both sides.

The TS-VC01 sampler now maintains approximately the same mix at every completed-request rate: 40% included changing codes, 20% excluded, 20% existing outside the ValueSet and 20% changing missing codes. Each consecutive five samples has that mix; finite-run totals can differ by at most four samples per VU. Both binaries use this same updated harness, and original preflight/response assertions are retained. TS-EX01 keeps its first/middle/last-page checks. Earlier increment throughput used a different iteration-dependent positive fraction, so its historical numbers are not substituted for the new matched baseline.

| Operation | VU | Previous RPS | Concurrent RPS | Ratio | Previous p50/p95/p99 ms | Concurrent p50/p95/p99 ms |
|---|---:|---:|---:|---:|---|---|
| TS-VC01 | 1 | 736.6 | 2336.7 | 3.17× | 1.031/2.353/4.237 | 0.343/0.486/0.956 |
| TS-VC01 | 10 | 1259.7 | 4978.2 | 3.95× | 7.208/11.191/17.338 | 1.749/2.739/5.773 |
| TS-VC01 | 50 | 1290.4 | 2326.0 | 1.80× | 36.317/50.649/62.754 | 13.785/52.299/80.610 |
| TS-EX01 | 1 | 863.1 | 1149.2 | 1.33× | 0.782/1.105/1.718 | 0.412/1.022/1.766 |
| TS-EX01 | 10 | 1184.2 | 2932.3 | 2.48× | 7.943/9.332/11.397 | 2.359/4.774/7.499 |
| TS-EX01 | 50 | 1059.4 | 3003.0 | 2.83× | 43.562/61.634/86.323 | 15.391/22.844/30.403 |

All twelve primary measurements have zero HTTP error rate and zero failed semantic checks. At 50 VU, membership throughput improves but p95 and p99 rise and the new throughput is lower than at 10 VU; the entire matrix is reported rather than selecting the peak. A separate repeat is recorded below.

| Operation | VU | Previous CPU % | Concurrent CPU % | Previous peak RSS MiB | Concurrent peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| TS-EX01 | 1 | 17.1 | 21.1 | 27.4 | 44.1 |
| TS-EX01 | 10 | 27.9 | 63.6 | 27.5 | 45.2 |
| TS-EX01 | 50 | 27.4 | 62.2 | 27.7 | 45.7 |
| TS-VC01 | 1 | 17.1 | 20.3 | 40.5 | 40.6 |
| TS-VC01 | 10 | 21.3 | 59.4 | 18.3 | 41.4 |
| TS-VC01 | 50 | 21.8 | 35.9 | 27.0 | 44.0 |

CPU is the interpolated server-process CPU-time delta (100% means one core), excluding database and load-client CPU. RSS is the maximum one-second sample during that measurement, not a hard peak or retained-plan size. Higher server CPU here accompanies more completed work; no per-request CPU improvement is inferred from the raw percentage.

The focused 50-VU membership verification uses the same 5-second warmup, 30-second duration and pool five; both repeats have zero HTTP/semantic errors. The after repeat was run first on the existing process, then the baseline was restarted for its repeat. These are diagnostic repetitions, not a randomized confidence interval, and do not replace the primary measurements.

| Repeat | RPS | p50 ms | p95 ms | p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|
| before | 888.2 | 52.709 | 92.602 | 150.058 | 21.4 | 47.2 |
| after | 4806.9 | 9.304 | 16.450 | 23.858 | 58.2 | 45.7 |

The new 50-VU membership result ranges from 2,326 to 4,807 RPS across the primary and repeat; baseline ranges from 888 to 1,290 RPS. Thus the primary high-concurrency latency regression is not consistently reproduced, and its cause is unresolved. Both results remain visible. More controlled repetitions are required before promising a high-concurrency tail-latency bound.

## Direct local FHIRsmith comparison

The product objective is to outperform FHIRsmith while preserving FHIR semantics; speedup against our previous binary alone does not establish that objective. After the paired term-squid runs, the retained unmodified FHIRsmith checkout `25a41f87711507789b361480ee4efae42df504d4` (0.18.2) was restarted locally with Node 24.16.0 and its existing folder configuration. Git status is clean. It loads the same synthetic resources and R4 core 4.0.1, with default reference caching and no source/worker changes. Only loopback `http://127.0.0.1:17003/tx/r4` receives load.

All seven original preflights were checked: six pass, while reverse translation retains the previously documented mismatch. Its assertion is unchanged and it receives no speed score. The two selected compose scenarios pass strict preflight and use the identical fixed membership mix, expansion pagination checks, 5-second warmup, 30-second duration and 1/10/50 VU as term-squid. Builds and profiling do not overlap these final reference measurements. This is a two-operation comparison, not the six-operation overall goal. Reference responses can contain richer metadata, so ratios describe these validated synthetic response contracts, not identical complete FHIR implementations.

| Operation | VU | term-squid RPS | FHIRsmith RPS | term-squid / reference | term-squid p50/p95/p99 ms | Reference p50/p95/p99 ms |
|---|---:|---:|---:|---:|---|---|
| TS-VC01 | 1 | 2336.7 | 1847.4 | 1.26× | 0.343/0.486/0.956 | 0.364/0.752/2.616 |
| TS-VC01 | 10 | 4978.2 | 3319.3 | 1.50× | 1.749/2.739/5.773 | 2.498/6.452/8.739 |
| TS-VC01 | 50 | 2326.0 | 3158.9 | 0.74× | 13.785/52.299/80.610 | 14.065/26.708/33.618 |
| TS-EX01 | 1 | 1149.2 | 582.1 | 1.97× | 0.412/1.022/1.766 | 1.417/1.903/2.470 |
| TS-EX01 | 10 | 2932.3 | 599.7 | 4.89× | 2.359/4.774/7.499 | 14.538/27.016/38.553 |
| TS-EX01 | 50 | 3003.0 | 726.3 | 4.13× | 15.391/22.844/30.403 | 70.195/93.984/108.753 |

All six reference load measurements pass with zero HTTP errors and zero failed semantic checks. In the primary matrix, term-squid expansion is faster at all three concurrency levels; membership is faster at 1/10 VU but slower at 50 VU. The repeat is disclosed separately rather than replacing that losing result. This is progress toward the reference goal, not a claim that the full goal is achieved.

| Operation | VU | Reference CPU % | Reference sampled peak RSS MiB |
|---|---:|---:|---:|
| TS-EX01 | 1 | 83.4 | 625.5 |
| TS-EX01 | 10 | 96.9 | 625.4 |
| TS-EX01 | 50 | 100.8 | 493.9 |
| TS-VC01 | 1 | 84.0 | 556.6 |
| TS-VC01 | 10 | 123.7 | 591.4 |
| TS-VC01 | 50 | 119.8 | 619.7 |

Reference CPU/RSS cover the Node server process. term-squid figures above exclude PostgreSQL, so process RSS is not a comparison of total product memory footprints. The richer reference metadata and supported-contract gaps must also remain visible when interpreting throughput.

The separate 50-VU reference repeat passes with 3032.9 RPS, p50/p95/p99 14.505/28.579/35.706 ms, server CPU 118.3% and sampled peak RSS 620.6 MiB. Its primary result was 3,158.9 RPS. term-squid's separate repeat is 4806.9 RPS (1.58× this reference repeat), but its primary result remains a loss. The reference is steadier in these limited repeats; stable high-concurrency superiority has not been demonstrated. The extra reference raw results are in `results/increment3-fhirsmith-repeat50/fhirsmith`.

## Limitations and next work

The generation SELECT still consumes a pool slot and network exchanges for every operation. Inspection of the locally installed SQLx 0.8.6 source confirms that pool checkout defaults to `test_before_acquire=true`; its PostgreSQL ping sends a protocol Sync and waits for readiness. Neither binary changes that default. This additional health exchange is a concrete optimization candidate, but its individual cost was not isolated in these measurements. Disabling it would also change connection-failure behavior and requires a separate test/measurement. Source excerpts are preserved in `profiles/increment3/sqlx-pool-health-source.txt`. The remaining database/HTTP scheduling cost is a candidate for the next bottleneck; this experiment changes both round-trip count and lock scope and cannot assign their individual speedups. Increasing the pool is a separate experiment, not part of this primary comparison. Database CPU, open-arrival-rate overload, larger root diversity and sustained mixed-operation workloads remain unmeasured here.

Cold work still serializes across distinct keys and rebuilds complete dependency CodeSystems. Unrelated content changes invalidate all plans. Shared immutable dependency snapshots and selective invalidation need a separate memory/accounting design and workload. Process RSS and queued requests are not bounded by the retained-cache charge. FHIR contract/clinical content limitations from prior reports remain. Neither the >12,000 simple-read target nor overall 3× reference target is established by these two local workloads; the direct FHIRsmith comparison above covers only these two compose workloads, not the full product suite.

## Reproduction and evidence

Raw evidence stays ignored under `.benchmarks/`: direct reference results are in `results/increment3-fhirsmith-matched/fhirsmith`, all reference preflights in `results/increment3-fhirsmith-preflight/fhirsmith`, high-concurrency verification in `results/increment3-repeat50-{before,after}/term-squid`; `results/increment3-matched-{before,after}/term-squid`, `results/increment3-after-preflight`, diagnostic-only `results/increment3-{before,after}-profile-only`, `profiles/increment3/`, instrumented source copies/binaries and `increment3-*.log`. Exact source archives and executable hashes are recorded in `profiles/increment3/sha256.txt`. The preserved baseline binary is `.benchmarks/backend-increment3-baseline` with SHA-256 `c017cc81ab1248f2687d7dea2a63aca4bf871d060b071fb65801120ea928d4c6`, matching the previous report. Both sources are dirty descendants of HEAD `3b5bb4ff67f038320d8bed551eeaf73239501169`; the manifests do not invent a committed source revision. The new executable SHA-256 is `89fbdc1b49f21f24cabd4e8c5d6180afb92812e76e67bf1e64426f7ac7561a26`.

Use the same current harness on both binaries:

```sh
python3 benchmarks/observe.py --pid SERVER_PID --output .benchmarks/UNIQUE-process.json -- \
  bun benchmarks/bench.ts run --tests TS-VC01,TS-EX01 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1'
```

The source snapshots are `profiles/increment3/before-source.tar.gz` (SHA-256 `db689a714599d978ed9da3130ddd6a78b50e535a3079bc1faffc3783a56aed6e`) and `profiles/increment3/after-source.tar.gz` (SHA-256 `82685728052ccf19824c20e2669c3cb3685d36fde4e6a6fdca7e74cf6aaf7a24`). Sequential laptop measurements are descriptive; randomized repetitions and a stable dedicated host are still needed for broader claims.
