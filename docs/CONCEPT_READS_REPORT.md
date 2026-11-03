# Prepared CodeSystem reads — November 3, 2026

At 50 VU, prepared lookup reaches **16,948.8 RPS** versus 5,818.5 baseline and 8,324.7 pinned reference; CodeSystem validation reaches **18,407.6 RPS** versus 5,630.5 baseline and 5,105.3 reference. Both exceed 12,000 on these synthetic warm workloads. A loaded lookup repeat reaches 21,265.1 RPS, so the primary is not replaced by the better peak. Single-client requests still lose to the reference; the overall 3x suite and clinical/mixed-load goals remain open.

## Implementation

`term-squid-core::PreparedConcepts` resolves provider-normalized comparison keys to immutable `ProjectedConcept` payloads. It owns stored spelling, display, definition and unchanged JSON properties. It accepts PostgreSQL-generated keys instead of substituting Rust Unicode lowercase, rejects duplicate/ambiguous rows and bounds concept count at 100,000. Payload/index memory charges account for strings and nested JSON using iterative traversal.

The backend now routes the existing `resolve_concept` and instance method through a prepared projection cache shared by `$lookup` and CodeSystem `$validate-code`. Metadata and bounded stored concept rows come from one read-only repeatable-read transaction. This is not preparation from CodeSystem resource JSON: fragment and legacy projections retain the same behavior as the previous SQL. The existing handlers and result model are unchanged, including display checks, definitions/properties, version output and the distinction between an absent code in complete content and unknown membership in incomplete content.

Warm requests use the existing bounded generation observer, including PostgreSQL query-key normalization. A request cannot join an observation detached before it arrived. The Concept family uses `hierarchies_tracked`, because migration `20261103000000` already installs concept-projection generation triggers. Each returned plan matches the observed generation; there is no TTL, cached generation reuse across later requests or stale-answer fallback on SQL failure. Cold misses recheck the generation in their own transaction. CodeSystem metadata and all selected concept fields therefore stay in one snapshot during concurrent updates.

One owned cold-admission mutex covers snapshot capture, blocking decode/build and publication. Warm hits bypass it. Cancellation of the awaiting HTTP future cannot release an active blocking builder's admission. Missing systems and preparation failures remain uncached. Cache publication is monotonic; old builds cannot replace newer generations.

## Limits and fallback

- At most **eight entries / 32 MiB** of conservative retained charges for this new cache. Existing ValueSet/ConceptMap/hierarchy budgets remain 64/16/16 MiB, for 128 MiB across the four caches.
- Raw preparation input is bounded at **32 MiB**, individual property JSON and admitted metadata at **64 KiB**, and concept count at **100,000**. Bounded aggregate checks happen before fetching rows.
- Property JSON remains text until blocking preparation. Each decoded payload is charged before it is accumulated; an over-budget object graph stops preparation before the full index is built. At most one individually bounded document can overshoot that estimate during decoding.
- Over-limit systems retain a generation-scoped SQL-admission marker rather than a partial index. Reads use the **unchanged atomic SQL**. Oversized external probes bypass preparation without truncation or false acceptance. This fallback preserves correctness, not a throughput guarantee; it may require an observation plus the SQL read.
- Final memory charges are computed outside the cache lock. Retained limits do not bound total RSS, transient rows/build buffers, response clones, old readers, cold waiters or SQL execution time. Known payloads are still cloned into the existing owned result type; this is not zero-copy lookup.

No migration or dependency is added. Apply all existing migrations before starting the new server and preserve transactional resource/concept writes and generation triggers. General observer admission remains 256 pending plus one active batch of at most 256; full live queues return HTTP 503. SQLx pool checkout and return health/recovery remain unchanged.

## Diagnostics and correctness

Before implementation, forced-generic `EXPLAIN (ANALYZE, BUFFERS)` probes for exact positive, exact missing and folded positive lookup already use exact/folded bitmap index bounds. Their isolated execution times are 0.138 / 0.033 / 0.064 ms. No whole-system scan is observed in these samples. Unlike the earlier subsumption issue, this increment does **not** claim to fix a demonstrated lookup scan or rewrite the fallback SQL. The target is reuse of stored projections and amortization of database/protocol/decoding work. These diagnostic timings exclude wire/client costs and are not a latency distribution or a causal CPU breakdown. Builds/diagnostics finish before final load.

All **47 Rust tests**, including ten opt-in database scenarios, pass. Workspace Clippy with warnings denied, formatting and twelve benchmark-harness tests pass. The release retains all seven local semantic preflights. These are regression checks, not full R4 conformance.

Five public-library tests cover exact/folded/missing probes, provider-defined Unicode keys, stored payload types, invalid/ambiguous rows, concept limits, memory charges and independent immutable ownership. Two database scenarios cover canonical/business versions, default/instance selection, GET/POST/display behavior, unchanged definition/property output, complete/fragment errors, rollback, projection-only commits/inserts, case-policy changes, atomic metadata/display updates, cancellation, eviction, schema guard, deletion and TRUNCATE. Large folded projections preserve PostgreSQL Unicode normalization. Separate count, oversized-property, decoded-object-memory and oversized-metadata cases must retain only SQL markers and still return correct payloads and post-commit updates. Existing generation-batch boundaries, overload, connection recovery and other operation assertions remain enabled.

## Measurement protocol

The baseline is the preserved [generation-batching build](GENERATION_BATCHING_REPORT.md), with prepared ValueSet/map/hierarchy reads but per-request SQL CodeSystem reads. Both binaries use explicit **pool 32**. Fresh processes run sequentially. The unchanged TS-CS01 sampler rotates existing codes and 20% missing probes; TS-LK01 checks changing display and business version across all 10,000 child codes. Each scenario gets one discarded 5-second warmup at 1 VU and 30-second windows at 1/10/50 VU. A separate 50-VU lookup repeat reverses server order. Every response must pass the original checker.

Reference: local FHIRsmith `25a41f87711507789b361480ee4efae42df504d4`, version 0.18.2 / txVersion 1.10.1, Node 24.16.0, default reference caching and the same bootstrap-only core pin/loopback patch. Both deployments contain synthetic 1.0.0, R4 core 4.0.1 (with the documented narrow legacy normalization on term-squid import), and the identified `hierarchy-folded#1.0.0` addition. Its SHA is in every dataset label; the original synthetic hash alone is not the complete database. Data does not change during timing. Reference reverse translation and folded hierarchy remain unscored semantic mismatches; they are not turned into successful workloads.

Host: Apple M3 Pro / 12 cores / 36 GiB / macOS 26.7.1; PostgreSQL 17.10 in the shared 2-vCPU / 4,095,365,120-byte Docker VM. Client: Bun 1.4.4 and k6 2.3.0. CPU/RSS use the one-second process observer (100% CPU = one core; RSS is sampled, excluding database/VM/client costs). This is a non-dedicated local deployment comparison, not language-only throughput or clinical performance. The other prepared operations receive semantic gates, not new throughput comparisons in this two-operation increment.

## Matched results

| Operation | VU | Previous RPS | Prepared RPS | Reference RPS | Prepared / previous | Prepared / reference |
|---|---:|---:|---:|---:|---:|---:|
| TS-CS01 | 1 | 1,451.1 | 1,551.6 | 3,067.9 | 1.069x | 0.506x |
| TS-CS01 | 10 | 4,837.6 | 7,297.9 | 5,413.2 | 1.509x | 1.348x |
| TS-CS01 | 50 | 5,630.5 | 18,407.6 | 5,105.3 | 3.269x | 3.606x |
| TS-LK01 | 1 | 1,373.6 | 1,495.4 | 3,877.1 | 1.089x | 0.386x |
| TS-LK01 | 10 | 4,572.0 | 7,340.9 | 8,432.9 | 1.606x | 0.871x |
| TS-LK01 | 50 | 5,818.5 | 16,948.8 | 8,324.7 | 2.913x | 2.036x |

At 50 VU, validation improves **3.269x** and lookup **2.913x** over the previous binary. Validation exceeds this reference **3.606x**, but lookup is only **2.036x**. No weighted overall score is calculated. At 1 VU both still lose to the reference. At 10 VU validation throughput wins but its p99 is worse than both baseline (5.004 versus 4.838 ms) and reference (3.537 ms); lookup throughput still loses to the reference. These points remain visible rather than being scored from only the high-concurrency peak.

All latency and resource points:

| Group | Server | Operation | VU | p50 / p95 / p99 ms | CPU % | Sampled RSS MiB |
|---|---|---|---:|---|---:|---:|
| main | baseline | TS-CS01 | 1 | 0.547 / 1.015 / 1.849 | 20.5 | 11.3 |
| main | baseline | TS-CS01 | 10 | 1.795 / 3.182 / 4.838 | 95.1 | 14.3 |
| main | baseline | TS-CS01 | 50 | 8.103 / 13.208 / 19.664 | 110.2 | 23.3 |
| main | baseline | TS-LK01 | 1 | 0.602 / 0.929 / 1.688 | 19.4 | 23.3 |
| main | baseline | TS-LK01 | 10 | 1.837 / 3.575 / 6.151 | 88.5 | 23.4 |
| main | baseline | TS-LK01 | 50 | 7.883 / 12.856 / 19.221 | 112.0 | 23.9 |
| main | candidate | TS-CS01 | 1 | 0.535 / 0.821 / 1.293 | 23.8 | 25.8 |
| main | candidate | TS-CS01 | 10 | 1.082 / 2.386 / 5.004 | 60.9 | 27.8 |
| main | candidate | TS-CS01 | 50 | 2.281 / 4.552 / 8.108 | 117.2 | 36.5 |
| main | candidate | TS-LK01 | 1 | 0.541 / 0.922 / 1.532 | 23.8 | 33.7 |
| main | candidate | TS-LK01 | 10 | 1.119 / 2.109 / 4.018 | 63.6 | 33.7 |
| main | candidate | TS-LK01 | 50 | 2.470 / 5.105 / 9.029 | 112.9 | 34.4 |
| main | reference | TS-CS01 | 1 | 0.257 / 0.349 / 0.658 | 67.7 | 605.2 |
| main | reference | TS-CS01 | 10 | 1.754 / 2.384 / 3.537 | 104.8 | 605.9 |
| main | reference | TS-CS01 | 50 | 9.333 / 15.465 / 19.213 | 103.4 | 609.9 |
| main | reference | TS-LK01 | 1 | 0.184 / 0.256 / 0.585 | 55.2 | 611.2 |
| main | reference | TS-LK01 | 10 | 0.959 / 1.943 / 4.126 | 102.8 | 551.4 |
| main | reference | TS-LK01 | 50 | 5.340 / 10.424 / 15.674 | 102.1 | 558.9 |
| repeat | baseline | TS-LK01 | 50 | 8.306 / 13.499 / 18.543 | 107.4 | 23.3 |
| repeat | candidate | TS-LK01 | 50 | 1.991 / 3.919 / 6.880 | 125.3 | 36.6 |
| repeat | reference | TS-LK01 | 50 | 5.163 / 9.271 / 14.035 | 105.1 | 421.2 |

The separate reverse-order 50-VU lookup repeat records **5,559.7 baseline / 21,265.1 prepared / 8,842.0 reference RPS**: 3.825x baseline / 2.405x reference. Prepared p99 is 6.880 versus 18.543 baseline and 14.035 reference ms. The prepared primary/repeat range is **16,949–21,265 RPS** on this host. Keep both; sequential measurements are not confidence intervals or a stable tail-latency guarantee.

The new index increases sampled Rust RSS: the primary loaded candidate reaches 36.5 MiB on validation and 34.4 MiB on lookup, versus 23.3/23.9 MiB baseline. Cache reuse trades retained payloads and cold allocator buffers for less repeated database work. RSS differences are not exact retained-plan charges, and database/VM memory is outside these columns. Rust-process CPU remains near the previous loaded level while more requests complete; this is not a whole-deployment CPU-cost measurement.

All **21 scored measurements** pass **4,399,050 response checks**, with zero HTTP errors and zero failed checks. Counts exclude warmup/preflight. The full reference preflight keeps its known reverse-translation failure, which receives no speed score. No checker, response field or expected error is removed to obtain these results.

## Reproduction and provenance

Apply all existing migrations and load the dataset above. Preserve the generation-batching release as baseline; build a separate candidate. Start each term-squid with explicit `DATABASE_MAX_CONNECTIONS=32`, `SERVER_HOST=127.0.0.1`, port 18081 and request logging disabled. Use fresh processes, identical local harness and the pinned reference/content setup. Never load-test public terminology servers.

```sh
python3 benchmarks/observe.py --pid SERVER_PID --output .benchmarks/UNIQUE-process.json -- \
  bun benchmarks/bench.ts run --tests TS-CS01,TS-LK01 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import) + hierarchy-folded#1.0.0 (bddb882a51243619048630da3095432f767d54d9db3da2bf25f7b485096c1393)'
```

Repeat loaded lookup with `--tests TS-LK01 --vus 50`, in reference/candidate/baseline order. For the reference add `--name fhirsmith --url http://127.0.0.1:17003/tx/r4 --server-revision 25a41f87711507789b361480ee4efae42df504d4`. Run all-scenario semantic preflights separately; do not score a failing scenario. The primary order is baseline, candidate, reference; each group is a fresh process with validation before lookup.

Identifiers:

- Baseline binary SHA-256: `0d6a837013d671f4818e10a06a391f25a53c405a9ea4c394859630eefcf47102`.
- Prepared binary SHA-256: `b1eabe489b7a5de0ff69ab233dd1380dd2cf149aa57c3adc81d6ddd93fa3b244`.
- Implementation source archive before final documentation: `7f203d6957843acfc5910481b43151fab305632b970d22b55b180a88a3b4fc0a`.
- Original synthetic SHA-256: `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`.
- Unchanged local harness SHA-256: `732026ed5adb0e59bf9fa50c46568c348ea6d262bff84dd866a34510aaba8399`.
- Additional folded fixture SHA-256: `bddb882a51243619048630da3095432f767d54d9db3da2bf25f7b485096c1393`.

Raw SQL diagnostics, settings, source archive, process samples, preflights, logs and summary remain ignored under `.benchmarks/profiles/concept-reads/` and `.benchmarks/results/concept-*`. These artifacts/binaries do not travel with Git; regenerate the content and rebuild on a clean machine. Earlier reports remain historical evidence, not substituted baselines. No commit, push or deployment was performed.

## Remaining work

Prepared CodeSystem reads still wait for fresh database observations. Low-concurrency reference latency, shared resolved-system snapshots, dependency-specific invalidation, cold/import/update costs, clinical content, mixed workloads, fixed-arrival-rate overload and response-memory/execution budgets need separate work. Large fallback systems and heavy properties are correctness-tested, not given a new performance guarantee.

The existing FHIR handlers are deliberately unchanged. This increment does not add richer typed properties, Coding/CodeableConcept inputs, language, lifecycle, supplements or broader provider semantics. Connect selected ecosystem tests and a requirements matrix to CI before treating synthetic success as release conformance. The product's overall 3x objective still has no agreed aggregation method; do not combine favorable points across reports into a manufactured score.
