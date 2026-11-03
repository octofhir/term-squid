# Subsumption pool-capacity follow-up — November 3, 2026

> Historical configuration: the measurements below use explicitly recorded capacities. The server now defaults to 32 after the [six-workload default selection](POOL_DEFAULT_REPORT.md). Set `DATABASE_MAX_CONNECTIONS=5` when reproducing a five-connection series.

## Outcome

A larger pool improves this workload, but does not close the reference gap. At 50 VU, pool 32 reaches 5,471 / 4,948 RPS in two fresh runs, versus 3,268 / 3,202 with pool five. Pinned FHIRsmith reaches 10,146 RPS. **The production default remains five**: a one-operation capacity experiment is not enough to select a new default for mixed workloads.

This follows the [atomic/indexed subsumption comparison](SUBSUMPTION_PERFORMANCE_REPORT.md). It uses exactly the selected `cfafe8f0f8099fe286622a280f7e19180aef164e4d919cc10cb01f2329952a3f` binary, the same four-outcome request mix, fixture/content versions, hardware and client. No source or data change is part of the capacity experiment.

## Separate profiling

Before final timing, a fresh pool-five process receives 15 seconds of validated traffic at 10 VU while macOS `sample` records ten seconds of stacks. All 49,659 diagnostic response checks pass. Its throughput and latencies are excluded from scored results because profiling is intrusive.

The sample contains PostgreSQL executor, SQLx pool-return and HTTP/I/O paths. Inspection of pinned SQLx 0.8.6 confirms that returning a pooled connection unconditionally calls `ping`; PostgreSQL implements that ping as a Sync/ReadyForQuery protocol exchange. The idle-aware checkout policy does not remove it. This is a source-level protocol fact, not a measured fraction of request latency.

SQLx uses that step both for health and to flush pending protocol/rollback work after cancellation. It is not removed or patched in production. Any alternative read path must preserve connection-state recovery, bounded admission and post-commit freshness, including cancellation/error cases.

A single PostgreSQL-container snapshot during the diagnostic records 39.54% CPU and 138.2 MiB of reported container memory. It is not a full database utilization series and cannot establish a precise bottleneck or total product memory cost.

## Matched capacity experiment

Fresh processes run in this order: five A, 32 A, reference, 32 B, five B. Every server passes the four-outcome preflight. Each run discards a 5-second warmup at 1 VU and then measures 30 seconds at 10 and 50 VU. The only Rust-server runtime change is `DATABASE_MAX_CONNECTIONS=5` versus `32`; configured limits are recorded beside each process series. No build or intrusive profile overlaps these windows.

| Run | VU | RPS | p50 / p95 / p99 ms | Server CPU % | Sampled server RSS MiB |
|---|---:|---:|---|---:|---:|
| Pool 5 A | 10 | 3,140.8 | 2.811 / 4.773 / 8.442 | 54.9 | 13.2 |
| Pool 5 A | 50 | 3,267.6 | 14.148 / 21.403 / 29.777 | 55.8 | 22.0 |
| Pool 32 A | 10 | 4,164.4 | 2.022 / 4.181 / 6.122 | 81.4 | 14.0 |
| Pool 32 A | 50 | 5,471.0 | 8.223 / 14.343 / 22.048 | 103.7 | 22.4 |
| FHIRsmith | 10 | 10,420.9 | 0.825 / 1.252 / 3.038 | 106.1 | 417.2 |
| FHIRsmith | 50 | 10,145.6 | 4.605 / 7.804 / 10.777 | 104.7 | 419.8 |
| Pool 32 B | 10 | 3,989.6 | 2.104 / 4.423 / 6.501 | 79.0 | 14.2 |
| Pool 32 B | 50 | 4,948.0 | 9.343 / 15.541 / 21.213 | 95.9 | 23.1 |
| Pool 5 B | 10 | 2,871.3 | 3.098 / 5.199 / 7.707 | 53.6 | 13.4 |
| Pool 5 B | 50 | 3,202.1 | 14.799 / 20.854 / 28.949 | 56.1 | 22.1 |

All ten sustained measurements pass: **1,548,962 response checks, zero HTTP errors and zero failed checks**. Pool 32 improves throughput and every measured latency percentile against its paired pool-five run, but still loses to the reference on both throughput and p99. The repeat is retained rather than replacing the primary pool-32 peak. Higher server CPU accompanies higher completed throughput; it is not an independent CPU-efficiency claim. PostgreSQL/VM/client costs remain excluded from the server CPU/RSS columns.

Pool capacity is a maximum, not a guarantee that every connection is active. More sessions also have database-side memory costs not represented by server RSS. Mixed/cold operations, cancellation and overload at arrival rates beyond capacity still need separate validation; this result does not justify a universal pool setting or establish the overall 3x reference / >12,000 simple-read targets.

## Evidence and reproduction

Raw settings, process series and summary JSON remain under ignored `.benchmarks/profiles/subsumption-pool/`; k6 results have `subsumption-pool-*` run IDs. The separate profile is under `profiles/subsumption/selected-active-cpu.txt`, with its diagnostic load and container snapshot beside it. The same source/fixture fingerprints are verified across all main and capacity runs.

Start the selected release with pool five or 32, keeping database and client unchanged:

```sh
DATABASE_MAX_CONNECTIONS=32 just bench-server
```

For each fresh server and the pinned local reference, run the same current harness:

```sh
bun benchmarks/bench.ts run --tests TS-SS01 --vus 10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import)'
```

Record the capacity with each artifact, observe process CPU/RSS with the benchmark README wrapper, and counterbalance order. Do not reuse the diagnostic profile's timing as a performance score or remove connection-health checks to inflate throughput.

## Next work

Test a bounded immutable hierarchy plan with a fresh generation check as the next engine candidate, comparing it with the selected SQL path and the pinned reference. Keep the operation on one snapshot, preserve explicit errors and case/version selection, and test cross-process commit/rollback/concurrent updates and cancellation. Evaluate data-query and connection-protocol costs separately; do not replace freshness with a TTL or silently disable SQLx recovery.
