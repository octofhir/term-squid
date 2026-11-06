# Native SNOMED ValueSet provider

This is the historical November 3 checkpoint. The later [native search increment](NATIVE_SEARCH_REPORT.md) implements its selected-dialect synonym text-search gap without changing RF2 projections. [Bounded mixed-provider composition](MIXED_VALUESET_REPORT.md) subsequently adds finite stored/temporary and explicit UCUM leaves. Other provider, corpus and historical performance limits below remain.

The later [ECL increment](ECL_RXNORM_RELATIONS_REPORT.md) adds bounded active-concept constraint/implicit queries, eight hierarchy operators and Boolean grouping. It supersedes the blanket ECL limitation below, not the remaining refset/refinement/history or clinical-performance limits.

The following [concept-refset increment](LOINC_PARTS_SNOMED_REFSETS_REPORT.md) adds explicitly declared complete Simple concept sets via `concept in`, implicit refset URLs and scalar ECL `^`. Global refset coverage, nonconcept/association semantics, refinements and history remain unsupported.

November 3, 2026. Based on `a016524`, with uncommitted implementation changes. This increment adds native RF2 ValueSet execution; it does not establish clinical composite leadership. The committed baseline rejected native compose dependencies, so there is no valid native throughput ratio against that unsupported implementation.

## Supported execution

The library now recognizes the supported R4 SNOMED implicit grammar and validates typed native filters. The PostgreSQL adapter resolves bounded definitions into a SQL set plan. Native operations return one membership or the requested page, rather than a release-sized JSON CodeSystem or finite `PreparedValueSet`. The common result is explicitly tagged: existing finite callers retain/borrow their shared immutable plan, while native callers receive an owned page. This avoids per-operation copies of finite member strings and root JSON during validation. A regression test verifies shared Arc identity and that an old plan remains unchanged after a committed update.

- Implicit `?fhir_vs` and `?fhir_vs=isa/<id>`, including explicit edition/version URIs. An unspecified implicit edition selects the latest International release; an unversioned ordinary compose keeps the existing latest-updated system resolution. Literal versions containing `%`/`_` never become edition wildcard selectors.
- Inline, stored canonical/version and instance operations; include/exclude, OR between selections, AND between filters, imported-set intersections, explicit displays and the finite engine's last-display duplicate policy.
- Active inferred `is-a`, `descendent-of`, `is-not-a` and `generalizes`; scalar code equality; selected projection properties (`inactive`, `moduleId`, `definitionStatusId`, `effectiveTime`); numeric relationship-property equality after verifying a bounded concept-model attribute ancestor path.
- `compose.inactive=false`, deterministic C-collation pages, filtered totals, zero count and offsets beyond the total, including `i64::MAX`. Unversioned whole-system exclusion removes every version, including codes absent from the latest release.

The supported implicit URL forms and International default follow [FHIR R4 SNOMED guidance](https://hl7.org/fhir/R4/snomedct.html). This is a supported subset, not full SNOMED/ECL conformance. ECL, refsets, other operators, mixed native/finite composition and synonym/dialect search beyond the projected display remain explicit limitations. Expansion text filtering is a literal case-insensitive code/display substring.

## Freshness and bounded work

Each native operation opens a fresh repeatable-read, read-only transaction. Root, imports, selected system IDs, properties, relationships, total and page belong to that snapshot. No native positive/negative result cache or generation TTL is used. An overlapping reader may finish on its old snapshot; a later reader sees committed changes. Rollback never changes the result. Existing generation-aware finite caches remain separate, and the finite adapter dispatches native dependencies instead of inventing empty systems.

Limits are enforced independently of requested page size: 4 MiB of definitions, 4,096 terms, 128 ValueSet definitions and 128 selections, depth 32 and sixteen hierarchy filters. Traversals are bounded at 8,192 nodes for membership and 1,000,000 for expansion. A bound violation rejects the whole result, including an early positive match. ValueSet API admission includes at most 64 requests before finite/native routing; at most four native application futures execute. The native phase has a fifteen-second deadline, SQL statements ten seconds, and PostgreSQL `work_mem` 4 MiB. Capacity/deadline failures return 503; unsupported work/size bounds return 422. Cancellation releases application admission immediately; SQLx then drains/rolls back the connection. A canceled PostgreSQL statement can continue until its server-side timeout, bounded by the pool and statement budgets. The tests do not establish immediate database-side cancellation or a four-statement database limit after repeated client disconnects; stronger server-work accounting remains operational work.

A conservative 32 MiB page charge gates JSON aggregation. The running charge suppresses oversized payload aggregation and the operation then returns an error; it never publishes a truncated successful page. PostgreSQL may sort/spill projected rows and retain bounded recursive IDs. `work_mem` is per SQL node, not a total database RSS guarantee. No full-release Rust cache is introduced.

Migration `20261103000002_snomed_children_index` adds the reverse active-inferred edge index. RF2 projection semantics and immutable source/context fingerprints are unchanged; a future semantic projection change still needs an explicit revision/rebuild workflow.

## Profiling and implementation decisions

The initial native candidate repeatedly joined the concept projection for membership, count and page. A 100,005-concept expansion profile showed about 400,020 concept-index probes plus 100,005 edge-index probes. Payload propagation removed the redundant projection join. Expansion now calculates total and page from one ordered stream, with running page charging before JSON aggregation. Membership walks ancestors of the requested code rather than enumerating descendants.

Exploratory SQL profiles showed a cardinality problem: a broad recursive set was estimated near 101 rows but contained 100,005. A forced hashed predicate improved one broad profile from 956 to 450 ms, but worsened a leaf from 0.241 to 14.311 ms. It was not selected unconditionally. The final candidate batches its fixed transaction setup statements into one protocol round trip without changing SQLx rollback ownership, isolation or LOCAL budgets. It also uses two exclusive SQL branches: up to 256 actual descendants use indexed probes, broader sets use a hashed predicate. The size and branch decision are computed once in the same snapshot. Boundary tests verify identical membership, exclusion, totals and ordering before and after a committed 256-to-257 change. The threshold is an initial execution policy, not a clinically calibrated optimum.

These are diagnostic profiles, not sustained throughput claims. Allocation counts were not measured in this increment; bounded response tests and process RSS do not prove the peak memory of every full clinical release. The next remaining cost is native SQL hierarchy traversal/search and per-operation metadata/transaction work. Compact immutable native indexes/prepared plans remain a separate optimization with the same freshness and memory requirements.

## Validation

The final suite has 73 passing Rust tests, including opt-in PostgreSQL integration; formatting and workspace Clippy with warnings denied pass. The unchanged benchmark harness has 29 passing tests. New generated fixtures cover positive, missing and excluded codes, imported intersections/cycles, active state, property conjunctions, explicit display duplicates, versions/editions, literal-version wildcard rejection, instance operations, page boundaries, more than 100,000 concepts, oversized pages, committed updates, rollback, snapshot consistency, cancellation, overload and non-partial traversal errors.

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
bun test benchmarks
```

The main corpus still has zero native clinical releases. Its unchanged local preflight passes 7/7; HealthSamurai remains 2/20 (FS01 and LK03). No full clinical composite or local Termbox/FHIRsmith speedup is claimed. [Acquisition sources and the upstream cache-version discrepancy](CLINICAL_DATA_SOURCES.md) are now documented separately.

## Matched generated diagnostic

The standalone probe uses a separate schema `native_valueset_probe_20261103`: 100,005 generated concepts, 100,004 edges in a ten-way tree, no clinical data and no benchmark-pool-only imports. Membership changes codes across 50% valid descendants, 25% existing nonmembers and 25% missing codes. Expansion filters `Generated 1`, verifies total 11,117, sorted distinct pages of ten and changing offsets including beyond the total.

Protocol: one global five-second mixed warmup at ten VU, thirty-second sustained runs at 1/10/50 VU and five-second pauses. Every response is semantically checked; failed thresholds remain failures. Builds, tests, SQL profiles and CPU sampling finish before timings. The final paired baseline already supports native sets and the one-pass stream; the candidate adds bounded cardinality-based execution and batched transaction setup. This measures that optimization, not a native-vs-unsupported baseline or the original clinical composite.

Host: Apple M1 MacBookPro17,1, eight CPUs, 16 GiB; OrbStack Docker VM eight CPUs / about 8 GiB; PostgreSQL 17; backend pool 32; native workers four; unchanged admission/time/page limits. Rust 1.99.0, Bun 1.4.3, k6 2.3.0. `ps` samples record backend CPU and RSS; database per-query CPU/RSS and production memory bounds are not established by those samples. k6 exported rates include its drain period; latency is in milliseconds.

| Operation | VU | RPS baseline → candidate | p50 ms | p95 ms | p99 ms | Failed / requests |
|---|---:|---:|---:|---:|---:|---|
| Membership | 1 | 770.71 → 955.75 | 1.20 → 0.96 | 1.49 → 1.20 | 1.77 → 1.47 | 0/23,123 → 0/28,674 |
| Membership | 10 | 1928.28 → 2377.56 | 4.95 → 3.99 | 6.50 → 5.29 | 7.78 → 6.36 | 0/58,066 → 0/71,337 |
| Membership | 50 | 1927.72 → 2031.87 | 25.12 → 21.43 | 30.92 → 39.39 | 46.46 → 54.97 | 0/57,878 → 0/60,994 |
| Expansion | 1 | 1.82 → 3.12 | 549.03 → 319.31 | 560.41 → 325.48 | 574.20 → 331.95 | 0/55 → 0/94 |
| Expansion | 10 | 6.31 → 10.81 | 1563.63 → 919.86 | 1902.12 → 1078.34 | 1930.42 → 1137.57 | 0/198 → 0/334 |
| Expansion | 50 | 6.26 → 10.65 | 7652.97 → 4668.65 | 8244.63 → 4819.09 | 8268.86 → 4871.29 | 0/238 → 0/368 |

All twelve points have zero HTTP and semantic failures. Expansion throughput improves about 1.7x. Membership improves at 1/10 VU; the primary 50-VU point improves throughput only modestly and regresses p99. Keep that regression visible rather than substituting another run.

| Operation | VU | Peak backend RSS MiB, baseline → candidate | Backend CPU core %, baseline → candidate |
|---|---:|---:|---:|
| find | 1 | 13.6 → 13.5 | 16.1 → 16.5 |
| find | 10 | 14.0 → 14.2 | 55.0 → 57.3 |
| find | 50 | 22.7 → 22.9 | 55.6 → 54.2 |
| expand | 1 | 22.7 → 22.9 | 0.1 → 0.2 |
| expand | 10 | 22.8 → 23.0 | 0.4 → 0.7 |
| expand | 50 | 23.1 → 23.0 | 0.4 → 0.7 |

CPU is derived from cumulative process CPU time over the sampled interval; 100% means one CPU core. These are backend-process observations, not database RSS/CPU or total-service memory guarantees.

Baseline artifact: `backend-single-pass`, SHA-256 `04661329acd1cf175970fd93764fc1a60f1916e22ca2d541316e11a819431161`.
Candidate artifact: `backend-shared-plans`, SHA-256 `62eed5254f6c20df24722fde4cd4345442cba221952e8af1eff2f1d5a022e16b`. Source archives: `single-pass-source.tar.gz` and `shared-plans-source.tar.gz`; final run labels: `one-pass-restored` and `shared-plans-restored`.

A separate short finite-path regression check used the unchanged TS-VC01/TS-EX01 requests at 50 VU, five-second warmup and ten-second runs, sequentially on the same main corpus. Borrowing finite plans improves membership 19,030 → 24,515 RPS and expansion 5,132 → 5,532 RPS, with zero HTTP/semantic errors. These short checks compare the intermediate cloning adapter with the final borrowing adapter; they are not clinical or sustained capacity claims. Raw runs are `native-finite-ownership-before` / `native-finite-ownership-after`.

The reverse-order membership repeat (candidate first, then baseline; same 50 VU/30s mix and warmup) records 1931.73 → 2366.04 RPS, p99 43.88 → 39.71 ms, with zero errors. Across primary/repeat points, candidate membership is 2,032–2,366 RPS and p99 39.71–54.97 ms, versus baseline 1,928–1,932 RPS and p99 43.88–46.46 ms. The primary tail regression is not reproduced, but there is no consistent 50-VU p99 improvement to claim. Both runs remain visible (`repeat-shared-plans`, `repeat-one-pass`).


Raw artifacts stay under `.benchmarks/native-valueset-20261103/`: fixtures, source archives/digests, immutable binaries/hashes, SQL plans, CPU sample, k6 summaries/logs and CPU/RSS samples. They do not travel with Git. Earlier experimental candidates are retained. The `after-final` cohort suffered an ENOSPC artifact-write failure and lost the OrbStack Docker socket; it is explicitly excluded. The database volume was restored without deletion, with the main corpus and generated schema intact. The final paired cohort was rerun after that restart. Original increment reports and artifacts are preserved.

Temporary backend processes used for these checks were stopped. The local PostgreSQL container/volume and isolated generated schema remain available; use the documented startup recipe for a new server process. No commit or push was performed.
