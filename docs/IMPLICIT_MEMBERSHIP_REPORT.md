# Implicit SNOMED membership: one data statement

November 3, 2026. This follows [native ValueSet execution](SNOMED_VALUESET_REPORT.md); both increments are working-tree changes over `a016524`. The scope is execution cost for supported implicit whole-system / `isa` membership. It adds no clinical corpus, new operators or composite coverage.

## Change and correctness contract

`term-squid-core` now exposes an immutable typed `ImplicitValueSet` definition. Existing JSON-template callers keep the compatible `implicit()` wrapper; dispatch no longer constructs JSON simply to recognize an implicit URL. Stored/inline/imported compose and expansion still use the general bounded provider.

For an implicit membership request, one recursive SQL data statement now performs release resolution, anchor validation, the bounded ancestor walk, candidate lookup and payload charging. The foreground transaction sequence falls from five whole-system round trips / six hierarchy round trips to four: BEGIN, the existing batched LOCAL setup, one data statement, COMMIT. This count excludes SQLx pool health/recovery traffic. This is not a transaction-free or database-free engine.

The resolver is shared with general native compose. Exact business versions, edition selection by release date, latest International default, deterministic ties and systemVersion precedence are preserved. Literal `%`/`_` versions never turn into SQL selection patterns. The selected release, concept, properties and graph use one fresh database snapshot; there is no new positive/negative/metadata cache, TTL or stale-error fallback. CLI commits and release activation are visible to later operations; overlapping readers retain a consistent snapshot.

The data statement preserves error ordering and the full 8,192-node completeness guard. A wrong system/version or early positive cannot bypass the traversal limit. Whole-system membership does not traverse relationships. Unknown anchors remain unsupported-expression errors; missing releases are not negative membership results; missing codes in a complete selected native release are negative results. Unsupported providers, expressions/ECL/refsets and general native limitations remain unchanged.

Payload charging happens only for a matching member. SQL CASE expressions mask oversized display/version/code fields before transfer to Rust. A nonmatching system/version does not trigger a display-budget error, while graph guards remain mandatory. The existing conservative 32 MiB charge, four native application futures, 64 common ValueSet admissions, pool default 32, ten-second SQL statements and fifteen-second native phase remain. Cancellation/database-drain limitations from the earlier report remain; this increment does not claim immediate SQL cancellation.

## Validation

All 81 Rust tests pass, including PostgreSQL integration; workspace Clippy with warnings denied, formatting and all 29 benchmark harness tests pass. Added tests cover typed URI/edition/version definitions, differential implicit-vs-inline Parameters, active/inactive and non-is-a edges, display checking, missing/foreign systems, conflicts, exact/edition/default resolution, later activation of an older edition, committed changes and rollback, stale-negative prevention, release replacement/deletion, concurrent complete version/display pairs, early positives beyond 8,192 nodes, and page-budget/error precedence with a 4.3-million-byte display. The final production binary passes 7/7 local preflights; unchanged upstream coverage remains 2/20 (FS01 and LK03). A separate static review found no actionable correctness issue.

No assertions or benchmark weights/pools were relaxed. No migration or RF2 projection change is required beyond the already documented native schema/index migrations.

## Allocation profiling

Isolated copies use the same System allocator probe as `benchmarks/profile_allocations.py`. Each mode has twenty warmup requests followed by 1,000 sequential changing codes. Whole-system requests are 75% existing / 25% missing; hierarchy requests are 50% descendants / 25% existing nonmembers / 25% missing. Every status, result, display and version is checked.

| Mode | Allocator calls / request, before → after | Requested bytes / request, before → after |
|---|---:|---:|
| Whole system | 254.653 → 175.347 (−31.1%) | 374,328 → 363,568 (−2.9%) |
| Hierarchy | 330.168 → 194.609 (−41.1%) | 382,061 → 365,313 (−4.4%) |

These include HTTP, runtime and SQL allocations. They measure allocation churn and requested sizes, not retained heap/RSS or allocator size classes. Instrumented time is not a throughput score. The relatively large remaining requested-byte count is not explained by this measurement alone; further allocation-size/stack profiling is needed before attributing it to a particular subsystem.

## Matched local diagnostic

The unchanged generated schema has 100,005 concepts and 100,004 tree edges. The same release/context and changing-code mix are used before/after, with version/display assertions. This is not clinical data or the HealthSamurai composite.

Protocol: one global mixed warmup of five seconds at ten VU; thirty-second windows at 1/10/50 VU; five-second pauses. Pool/concurrency/time/page budgets are unchanged. `ps` captures cumulative CPU and RSS. No builds or profiles overlap admitted production measurement windows. The initial `before` run overlapped a core build and is explicitly invalid; only `before-clean` is admitted as baseline. Both binaries use the same generated schema and explicit International 20260201 business version. Hierarchy requests use ancestor `2`; the code mix matches the allocation probe.

The host is an Apple M1, eight CPUs, 16 GiB RAM, macOS 26.5; PostgreSQL runs in local OrbStack, with client and server on the same host. Production binaries use Rust 1.99.0; the timing client is k6 2.3.0 and final harness checks use Bun 1.4.3. Host scheduling, thermal state and database caches are not isolated. The short global warmup does not establish stationarity at every point.

| Build | Mode / VU | RPS | p50 ms | p95 ms | p99 ms | Requests | Backend CPU % | Peak RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Before | isa / 1 | 892.9 | 0.949 | 1.685 | 3.084 | 26,788 | 16.9 | 13.2 |
| Before | isa / 10 | 2,360.7 | 3.888 | 5.930 | 8.983 | 70,830 | 57.3 | 13.2 |
| Before | isa / 50 | 1,454.3 | 25.886 | 81.934 | 164.215 | 44,075 | 42.5 | 22.0 |
| Before | whole / 1 | 571.7 | 0.923 | 4.404 | 10.789 | 17,153 | 16.0 | 22.0 |
| Before | whole / 10 | 469.9 | 16.169 | 58.284 | 88.017 | 14,108 | 23.2 | 22.3 |
| Before | whole / 50 | 419.7 | 104.643 | 215.167 | 258.399 | 12,638 | 21.6 | 22.3 |
| After | isa / 1 | 271.7 | 2.403 | 10.380 | 18.873 | 8,152 | 14.9 | 13.3 |
| After | isa / 10 | 2,750.5 | 3.298 | 5.117 | 9.263 | 82,527 | 56.3 | 13.1 |
| After | isa / 50 | 2,922.8 | 16.341 | 20.537 | 34.336 | 87,743 | 59.1 | 21.9 |
| After | whole / 1 | 1,208.4 | 0.717 | 1.019 | 1.388 | 36,252 | 17.8 | 22.0 |
| After | whole / 10 | 2,944.4 | 3.054 | 4.642 | 8.920 | 88,345 | 56.1 | 22.1 |
| After | whole / 50 | 2,814.5 | 16.773 | 23.332 | 37.488 | 84,502 | 54.2 | 21.7 |

CPU is backend process CPU-time delta divided by sampled wall time; 100% means one core. RSS is the maximum sampled backend resident size, not a complete memory bound. Database VM and client CPU/RSS are not captured in this matrix. All twelve points have zero HTTP failures and zero semantic failures.

The primary single-client hierarchy point regresses substantially. A same-process candidate repeat, then baseline and candidate restarts, retain the same mixed warmup and thirty-second windows:

| Series | Mode / VU | RPS | p50 ms | p95 ms | p99 ms | Requests | Backend CPU % | Peak RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Candidate, same process | isa / 1 | 1,319.8 | 0.644 | 0.972 | 1.670 | 39,596 | 16.8 | 21.4 |
| Baseline, restarted | isa / 1 | 924.1 | 0.912 | 1.462 | 2.871 | 27,725 | 16.9 | 13.9 |
| Baseline, restarted | whole / 50 | 2,424.7 | 18.099 | 31.700 | 55.122 | 72,794 | 55.2 | 23.0 |
| Candidate, restarted | isa / 1 | 1,039.8 | 0.668 | 1.517 | 6.384 | 31,210 | 15.5 | 13.5 |
| Candidate, restarted | whole / 50 | 3,087.6 | 15.236 | 20.168 | 33.646 | 92,682 | 59.3 | 23.1 |

The seventeen measured windows contain 837,120 requests with zero HTTP/semantic failures. The single-client hierarchy regression does not reproduce in these repeats; its cause remains unresolved. The restarted candidate improves hierarchy throughput 12.5% over the restarted baseline, while its p99 is worse (6.384 versus 2.871 ms). Baseline whole-system throughput varies from 420 to 2,425 RPS, so the primary apparent 6.7x win is not a stable speedup claim. The restart control gives a smaller 27.3% whole-system gain at 50 VU, with p99 55.122 → 33.646 ms. These observations support reduced execution overhead, not a guaranteed gain at every concurrency or a clinical ranking.

To investigate prepared-plan selection separately, the exact candidate data statement was captured and run with `EXPLAIN (ANALYZE, BUFFERS)` under forced custom and generic plans. The isolated samples report 1.955 / 0.943 ms planning and 0.399 / 0.225 ms execution respectively, with equal estimated cost. They do not explain the primary outlier or represent HTTP latency. No planner GUC was changed in production.

## Provenance and next bottleneck

The immutable baseline is the final shared-plan binary from the preceding increment, SHA-256 `62eed5254f6c20df24722fde4cd4345442cba221952e8af1eff2f1d5a022e16b`. The candidate is `5f188d315a9de220b8f281384565867a8c19b6b185f1466b3b78b3d086fe122a`. Candidate source archive SHA-256 is `743908ed9179ca03bf87bd2062d2352f493268e187407e4f3c2c5c2881153ffd`; all 77 Rust/manifests/migration inputs listed in the source manifest were verified unchanged after testing. Documentation was completed after that archive.

Remaining measured overhead includes four foreground transaction round trips and about 360 KiB of requested allocation per HTTP request. General native compose still prepares bounded metadata; graph evaluation remains recursive SQL, and expansion visits/orders matching projections. Further profiling should locate allocation sizes/stacks and separate connection/transaction wait from SQL execution on a stable host before selecting compact immutable native indexes or further transaction changes. This increment does not measure their prospective benefit. The clinical gaps and incomplete full-corpus ingestion remain the next coverage work; they cannot be resolved by this generated timing result.

Raw source snapshots, immutable production/instrumented binaries and SHA-256, driver, summaries, logs and CPU/RSS samples stay under `.benchmarks/implicit-membership-20261103/`. Earlier native reports/artifacts are preserved. No local FHIRsmith/Termbox clinical speedup or first-place score is established; full clinical content and a complete matched cohort remain required.
