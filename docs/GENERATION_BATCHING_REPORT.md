# Fresh generation batching — November 3, 2026

At 50 VU, batching reaches **18,157 subsumption RPS** versus 6,524 baseline and 10,120 pinned-reference RPS. ValueSet membership reaches 19,648 and forward translation 18,228 RPS. The >12,000 level is now demonstrated on these three synthetic warm prepared reads, not on every simple read or clinical workload. The overall 3x reference objective remains unachieved: subsumption is only 1.79x and forward translation 2.62x the reference; low-concurrency subsumption still loses.

## Implementation

Prepared ValueSet, ConceptMap and hierarchy reads now share a bounded observer in `crates/backend/src/store/generation.rs`. It amortizes warm generation queries across overlapping requests, without retaining a completed generation or using a TTL.

One worker acquires a normal SQLx pool connection, detaches the currently queued requests, then executes one autocommit statement. The statement reads the transactional generation and family schema guards and folds hierarchy query keys with PostgreSQL `lower()`. A request arriving after detachment cannot join that batch, even while its SQL response is still in flight. It waits for a later statement whose snapshot starts after its admission. Thus a request starting after an independently committed update cannot borrow an older in-flight observation.

Replies carry the observed generation into the existing monotonic immutable caches. Cold misses still recheck generation, selected resources and dependencies in their own repeatable-read transactions. No cache lock spans SQL. Version/instance selection, hierarchy normalization, unsupported-operation errors, existing cache/preparation budgets and atomic projection tracking are unchanged. CodeSystem lookup/validation still use their separate SQL path.

The worker is independent of the initiating HTTP future. Cancelling a recipient does not abort SQL or release cold preparation prematurely. Cancelled queued recipients are pruned before dispatch and when reclaiming a full queue. Completed responses are not kept for later callers. Idle workers exit; worker teardown clears pending senders rather than leaving a permanently active queue.

Limits and failures:

- At most **256 pending requests plus one active batch of at most 256** per store. There is one observer worker, not a task per queued request or a growing pending-key map.
- Each hierarchy probe admits at most 4,096 bytes per query code, preserving the existing fresh-SQL bypass for oversized probes. Submitted code buffers in pending plus active work are therefore bounded by 4 MiB, excluding response buffers, HTTP inputs and immutable plans.
- A full live queue returns **HTTP 503 OperationOutcome** with `Service overloaded`; it does not wait in another hidden queue or return stale data. This is a prepared-observation limit, not a global HTTP/SQL execution budget.
- Database/query failures are shared by every affected recipient and return errors, never cached answers. Family guards fail only their own family within a successful mixed batch. Missing generation rows or required columns fail the entire query.
- SQLx checkout policy and return-to-pool health/recovery remain intact. Each batch returns its connection normally. No pool-size, TCP policy or SQLx dependency change is used.

No new migration is introduced. All existing migrations, including `20261103000000_hierarchy_generation`, must already be applied; the shared query requires the map/hierarchy tracking columns and triggers. This is an amortized database-dependent path, not Milestone 2's database-free objective.

## Correctness

All **40 Rust tests**, including eight opt-in database scenarios, pass. Workspace Clippy with warnings denied, formatting and twelve benchmark-harness tests pass. The release retains all seven local semantic scenarios. These are regression checks, not full FHIR conformance.

New public store/HTTP tests hold delivery of a completed old observation, commit a projection update through an independent SQL session, then enqueue a new request. The old operation may finish using its original plan; the new operation must see the committed update, and counters require a second query. Additional checks cancel an active recipient, batch mixed folded probes and ValueSet reads, cancel a member after key normalization, fill all 256 slots, require HTTP 503 on the next request, reclaim a cancelled slot and verify later recovery. A renamed generation column forces a warmed operation to fail rather than return stale data; restoring it allows the next request to succeed. Mixed-family guards reject map/hierarchy reads while allowing ValueSet reads from the same batch. Existing atomic update, rollback, deletion, default-version, cache eviction, oversized-plan and lost-connection recovery assertions remain enabled.

## Measurement protocol

The baseline is the preserved [prepared-hierarchy build](HIERARCHY_REPORT.md), not an old five-connection SQL implementation. Both binaries use explicit pool **32**. Builds/checks finish before timing; servers run sequentially as fresh processes. The pinned local FHIRsmith revision is `25a41f87711507789b361480ee4efae42df504d4`, version 0.18.2 / txVersion 1.10.1, Node 24.16.0, default reference caching and the same bootstrap-only package pin/loopback patch. No public server receives load.

The unchanged harness measures four affected workloads at 50 VU: TS-VC01 membership, TS-EX01 pagination, TS-SS01 four-outcome hierarchy, and TS-CM01 forward translation. Subsumption also runs at 1/10 VU and repeats at 50 VU in reverse server order. Each scenario gets one discarded 5-second warmup at 1 VU; measured windows last 30 seconds. Every response must pass the original semantic checker. Reference reverse translation and folded hierarchy remain unscored because of their preserved semantic mismatches.

Both deployments contain synthetic 1.0.0, R4 core 4.0.1 (with the documented legacy normalization on term-squid import), and the separately identified `hierarchy-folded#1.0.0` resource from the previous increment. The extra fixture SHA is recorded in the dataset label; the unchanged original synthetic fingerprint alone does not describe the whole database. No dataset mutation occurs during timing.

Host: Apple M3 Pro / 12 cores / 36 GiB / macOS 26.7.1. PostgreSQL 17.10 uses the shared 2-vCPU / 4,095,365,120-byte Docker VM. Client: Bun 1.4.4 and k6 2.3.0. CPU/RSS use the one-second server-process observer: 100% CPU is one core; sampled RSS excludes PostgreSQL/VM/load-client costs and is not a hard peak. This non-dedicated local comparison does not establish language-only throughput, clinical-scale performance or an overall weighted product score.

## Loaded results

| Operation, 50 VU | Previous RPS | Batched RPS | Reference RPS | Batched / previous | Batched / reference |
|---|---:|---:|---:|---:|---:|
| TS-VC01 | 6,819.7 | 19,647.7 | 3,303.3 | 2.881x | 5.948x |
| TS-EX01 | 4,996.8 | 8,738.6 | 1,012.9 | 1.749x | 8.628x |
| TS-SS01 | 6,524.0 | 18,156.7 | 10,119.8 | 2.783x | 1.794x |
| TS-CM01 | 6,480.1 | 18,228.2 | 6,963.6 | 2.813x | 2.618x |

Every affected workload improves over the previous build at 50 VU (1.75–2.88x), with lower p99. All four also exceed this local reference's throughput at 50 VU. This does not establish an overall suite score: lookup/CodeSystem validation are unchanged, no weighting method has been agreed, and the low-concurrency losses below remain visible.

| Operation | Previous p50 / p95 / p99 ms | Batched p50 / p95 / p99 ms | Reference p50 / p95 / p99 ms |
|---|---|---|---|
| TS-VC01 | 6.853 / 10.294 / 13.681 | 2.213 / 4.056 / 7.062 | 14.114 / 24.422 / 34.337 |
| TS-EX01 | 9.356 / 13.267 / 15.890 | 3.794 / 8.982 / 12.643 | 50.369 / 59.042 / 74.820 |
| TS-SS01 | 7.104 / 10.872 / 16.335 | 2.346 / 4.548 / 8.440 | 4.728 / 7.685 / 9.460 |
| TS-CM01 | 7.205 / 10.995 / 13.401 | 2.313 / 4.645 / 7.289 | 6.676 / 11.328 / 15.079 |

| Operation | Previous CPU % / RSS MiB | Batched CPU % / RSS MiB | Reference CPU % / RSS MiB |
|---|---|---|---|
| TS-VC01 | 126.4 / 49.4 | 126.1 / 51.2 | 113.9 / 650.0 |
| TS-EX01 | 128.5 / 51.5 | 133.1 / 53.9 | 103.4 / 662.4 |
| TS-SS01 | 120.1 / 52.0 | 117.1 / 54.1 | 106.2 / 662.6 |
| TS-CM01 | 119.1 / 52.1 | 116.6 / 54.3 | 102.9 / 662.8 |

Rust-process CPU stays near the previous level while more responses complete; this excludes database/client costs and is not a total CPU-efficiency measurement. Main-process RSS includes all four warmed plan families, unlike the separate subsumption-only processes below.

## Unloaded points and reverse-order repeat

| Group | Server | VU | RPS | p50 / p95 / p99 ms | CPU % | RSS MiB |
|---|---|---:|---:|---|---:|---:|
| low | baseline | 1 | 1,656.5 | 0.511 / 0.702 / 1.035 | 21.4 | 14.2 |
| low | baseline | 10 | 5,432.2 | 1.622 / 2.641 / 4.172 | 101.7 | 17.1 |
| low | candidate | 1 | 1,612.5 | 0.527 / 0.711 / 1.078 | 23.6 | 14.4 |
| low | candidate | 10 | 7,670.6 | 1.123 / 1.750 / 4.064 | 65.1 | 16.5 |
| low | reference | 1 | 4,253.2 | 0.169 / 0.234 / 0.650 | 54.2 | 412.5 |
| low | reference | 10 | 10,445.9 | 0.823 / 1.345 / 2.958 | 104.4 | 337.7 |
| repeat | baseline | 50 | 6,629.6 | 6.926 / 10.750 / 17.938 | 122.8 | 25.0 |
| repeat | candidate | 50 | 17,487.4 | 2.358 / 5.033 / 9.295 | 112.2 | 25.6 |
| repeat | reference | 50 | 9,963.9 | 4.640 / 8.044 / 11.729 | 105.8 | 419.3 |

At 1 VU there is no overlap to share: the batched build falls **2.7%** in throughput and its p99 rises **1.035 → 1.078 ms** in this pair. This small losing point is not replaced by a peak or claimed as a statistical regression estimate. At 10 VU it improves throughput **41.2%** over the previous build but still loses to the reference (7,671 versus 10,446 RPS).

The independent 50-VU reverse-order repeat reaches **17,487.4 RPS**, versus 6,629.6 previous and 9,963.9 reference: **2.638x** previous / **1.755x** reference. Batched p99 is 9.295 versus 17.938 previous and 11.729 reference ms. Both loaded results are retained, not averaged into a best-peak claim. These sequential repetitions on a non-dedicated host are not confidence intervals or a tail-latency guarantee.

All **21 scored measurements** pass **5,284,983 response checks**, with zero HTTP errors and zero failed checks. Counts exclude warmup/preflight. The full reference preflight still fails reverse translation; that failure is preserved and receives no throughput score. No semantic assertion or response requirement was weakened.

## Reproduction and provenance

Apply all repository migrations and load the dataset described above. Preserve the previous hierarchy release as baseline and build a separate candidate. For each term-squid process set `DATABASE_MAX_CONNECTIONS=32`, `SERVER_HOST=127.0.0.1`, port 18081 and request logging off. Use fresh processes and identical fixture/harness hashes on the pinned local reference.

```sh
python3 benchmarks/observe.py --pid SERVER_PID --output .benchmarks/UNIQUE-process.json -- \
  bun benchmarks/bench.ts run --tests TS-VC01,TS-EX01,TS-SS01,TS-CM01 \
  --vus 50 --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import) + hierarchy-folded#1.0.0 (bddb882a51243619048630da3095432f767d54d9db3da2bf25f7b485096c1393)'
```

For the unloaded group use `--tests TS-SS01 --vus 1,10`. Repeat the 50-VU TS-SS01 group in reverse server order. The original sequence is baseline-low, baseline-loaded, candidate-low, candidate-loaded, reference-low, reference-loaded, then reference/candidate/baseline subsumption repeats. Each invocation gets a fresh server. Use `--name fhirsmith --url http://127.0.0.1:17003/tx/r4 --server-revision 25a41f87711507789b361480ee4efae42df504d4` for the reference. Run all-scenario semantic preflights separately; do not score its failing reverse scenario.

Identifiers:

- Baseline binary SHA-256: `67913ae612e58e3addd804688d6457718b1a5def5c619c6d347f18d3b6e2023e`.
- Batched binary SHA-256: `0d6a837013d671f4818e10a06a391f25a53c405a9ea4c394859630eefcf47102`.
- Implementation source archive before final documentation: `1ab9c7791b01202e63933b6cdb9f625f4338aff1b00c6fffded51b79dd0d0354`.
- Original synthetic SHA-256: `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`.
- Unchanged local harness SHA-256: `732026ed5adb0e59bf9fa50c46568c348ea6d262bff84dd866a34510aaba8399`.
- Additional folded fixture SHA-256: `bddb882a51243619048630da3095432f767d54d9db3da2bf25f7b485096c1393`.

Raw settings, binaries/source snapshot, process samples, preflights, logs and derived summary remain under ignored `.benchmarks/profiles/generation-batching/` and `.benchmarks/results/generation-*`. They do not travel with Git; regenerate content and rebuild on a clean machine. Previous hierarchy/default-capacity reports remain historical evidence, not substituted baselines. No commit, push or deployment was performed.

## Remaining work

CodeSystem lookup/validation retain their separate per-request SQL path. Prioritize their measured scan/protocol costs before extrapolating this result to all simple terminology reads. Prepared families still require fresh database observations and cannot serve as a database-free engine/API design.

Cold preparation/import/update costs, dependency-specific generations, large clinical content, mixed workloads, fixed-arrival-rate overload, worker/queue observability, request budgets and stable-machine repetitions remain incomplete. Batching intentionally allows 256 active plus 256 queued observations, but does not bound cold-plan waiters, general HTTP input memory or arbitrary SQL time. Clinical conformance, low-concurrency reference performance and an agreed overall 3x target remain open.
