# Prepared ConceptMap increment — November 2, 2026

## Scope and baseline

At the user's request, the accumulated previous work was committed as `839e14bdebc7fe5b5f4991c5662df62f5b051c1d` and pushed to `origin/main` without a coauthor trailer. This report covers subsequent uncommitted translation work. Earlier reports and raw artifacts are preserved. The objective remains outperforming local FHIRsmith while preserving semantics, not only beating our previous build.

The preceding pool experiment exposed a translation regression. Its source read and decoded the entire ConceptMap for each operation and then scanned groups/elements/targets, in both directions. This increment removes that repeated preparation from the warm operation rather than reverting or weakening semantic assertions.

## Library plan and execution

`term-squid-core::PreparedConceptMap` owns immutable group data, shared string buffers, forward and reverse code indexes, and ordered mapping entries. Preparation occurs once per retained resource/generation. Queries borrow matched fields; the FHIR adapter only allocates the actual response. There is no per-code answer cache, so changing and missing-code traffic cannot grow cache cardinality.

The indexes preserve group/target ordering, duplicate matches, target-system filtering, source display on reverse results and inversion of wider/narrower/subsumes/specializes. Unmatched/disjoint targets never become positive results. Conditional dependsOn/product mappings retain the existing explicit error scope, including the conservative reverse-group behavior. Unmapped fallback modes are now rejected explicitly when fallback evaluation would be required; a known explicit match does not need that fallback. The earlier evaluator silently ignored those modes. Source/target version context, richer mapping properties and full R4/R5 conformance remain outside the implemented subset; this does not claim broader mapping support.

HTTP GET/POST, canonical/business-version selection and instance UUID operations share the prepared path. The resource read/search endpoints keep their original resource-returning path. Missing maps and versions return an error and are not cached. Default selection is deterministic by updated_at and UUID; a versioned request never falls back to another business version.

## Consistency, limits and migration

Migration `20261102000003` attaches the existing transactional generation protocol to ConceptMap INSERT/UPDATE/DELETE/TRUNCATE. It also adds the `maps_tracked` schema guard: the prepared path requires it in its generation query. A new binary against an old schema therefore fails closed instead of silently caching maps whose CLI writes are not tracked. Apply this migration before running the new build; normal server startup still does not apply migrations.

Warm operations perform a fresh generation SELECT and reuse only exactly matching plans. A miss uses its own repeatable-read transaction for generation and resource resolution. A separate owned admission mutex serializes map preparation and remains held during blocking CPU work even if the awaiting request is cancelled. Cache locks do not span SQL/preparation. Generation observations and publication only advance; old in-flight plans can finish without replacing newer cached generations. Separate CLI processes participate through the database trigger; there is no notification lag or generation TTL.

The ConceptMap cache retains at most eight plans and 16 MiB of conservative memory charges. The existing ValueSet cache remains at eight plans/64 MiB, so combined retained-cache charges are bounded at 80 MiB. Oversized plans are evaluated without retention. Input is limited to 32 MiB, 128 groups and 100,000 target entries. Shared source/code strings avoid duplicating large source metadata for every edge. These limits do not establish a hard process RSS, response-size or HTTP request-queue budget; in-flight Arc handles and transient parsing can outlive eviction.

All relevant table updates conservatively invalidate all prepared generations; selective dependency invalidation is still future work. Database owners must preserve the generation triggers and write resources atomically. The unpublished migration was reverted/reapplied only on the local benchmark database while adding the schema guard; its resource rows and retained volume were preserved.

## Validation and measured preparation costs

All 26 Rust tests, including three PostgreSQL integration scenarios, pass; workspace Clippy with warnings denied and formatting pass. Nine benchmark harness tests pass. New integration assertions cover sixteen concurrent requests sharing one Arc, canonical/version conflicts, explicit/default versions, instance calls, positive and negative changes in both directions, uncommitted updates, rollback, insertion after a failed lookup, deletion, TRUNCATE and eviction without invalidating existing readers. The schema-guard test forbids reuse of an already warm plan when write tracking is unavailable. Existing tests and original semantic preflight assertions remain intact.

The baseline active CPU sample contains SQLx JSON decoding, serde map construction and translation traversal. The final warm CPU sample, collected separately after preparation, contains generation-query/HTTP work and indexed translation without the repeated full-map JSON decoder stacks. Profiles indicate executed stacks, not exact individual-stage wall times. SQL sizing/JSON transfer and cold plan construction remain on a cache miss.

An isolated instrumented build performs the same 20 warmup and 1,000 changing-code forward translation requests on both sides, including 20% missing codes and result assertions:

| Per request | Pushed baseline | Final prepared build |
|---|---:|---:|
| Allocation/reallocation calls | 1,676.753 | 105.729 |
| Requested allocation bytes | 572,052.028 | 346,112.075 |

These are process-wide deltas including HTTP, runtime, SQL and probe overhead. Requested bytes measure churn, not retained memory. Instrumented timing is excluded from throughput scores. The remaining requested-byte floor is not attributed to a particular subsystem without a separate measurement. Prototype profiles/results are retained separately; the table uses the final schema-guarded build.

## Matched sustained measurements and local reference

The current translation harness keeps the original positive probes and adds a missing-code probe. Load changes codes across the 128 mappings and injects 20% changing missing codes in each direction. Both our binaries and the reference use this same harness; historical positive-only throughput is not substituted for the new baseline. The reference's positive and missing forward probes were explicitly verified before load.

Final runs use the same retained synthetic package 1.0.0 plus R4 core 4.0.1, PostgreSQL 17, default pool five, Apple M1 host, loopback, 5-second discarded warmup and 30-second sustained runs at 1/10/50 VU. Previous/new servers run forward and reverse; only passing forward translation is scored for FHIRsmith. Its reverse mismatch remains visible in all-scenario preflight and receives no speed score. Builds and intrusive profiling finish before final measurements. Server processes run sequentially, and one-second process CPU/RSS sampling uses the same observer.

| Operation | VU | Baseline RPS | Prepared RPS | Ratio | Baseline p50/p95/p99 ms | Prepared p50/p95/p99 ms |
|---|---:|---:|---:|---:|---|---|
| TS-CM01 | 1 | 1821.4 | 1759.9 | 0.97× | 0.410/0.784/1.432 | 0.346/0.892/3.082 |
| TS-CM01 | 10 | 3364.7 | 5385.3 | 1.60× | 2.504/4.833/8.539 | 1.444/3.234/6.703 |
| TS-CM01 | 50 | 3404.1 | 6343.5 | 1.86× | 13.194/23.384/31.838 | 6.900/13.176/19.724 |
| TS-CM02 | 1 | 1395.5 | 2637.7 | 1.89× | 0.588/0.972/1.484 | 0.232/0.571/1.554 |
| TS-CM02 | 10 | 3530.1 | 1767.3 | 0.50× | 2.379/4.641/8.652 | 4.519/10.876/23.838 |
| TS-CM02 | 50 | 3300.8 | 1715.4 | 0.52× | 13.830/22.303/31.869 | 25.212/52.882/90.955 |

All twelve term-squid sustained measurements have zero HTTP errors and zero failed semantic checks. Forward translation improves at 10/50 VU, but 1-VU forward and 10/50-VU reverse regress in the primary matrix. The counter/allocation improvement does not establish universal HTTP speedup; these losing points are retained.

| Operation | VU | Baseline CPU % | Prepared CPU % | Baseline peak RSS MiB | Prepared peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| TS-CM01 | 1 | 30.3 | 20.0 | 40.7 | 14.2 |
| TS-CM01 | 10 | 85.4 | 49.4 | 42.0 | 16.4 |
| TS-CM01 | 50 | 81.0 | 63.2 | 44.8 | 22.2 |
| TS-CM02 | 1 | 31.1 | 21.3 | 22.4 | 22.3 |
| TS-CM02 | 10 | 86.5 | 33.1 | 16.2 | 21.7 |
| TS-CM02 | 50 | 87.2 | 32.7 | 23.1 | 21.8 |

| VU | Prepared forward RPS | FHIRsmith forward RPS | Prepared / reference | Prepared p50/p95/p99 ms | Reference p50/p95/p99 ms | Reference CPU % | Reference peak RSS MiB |
|---:|---:|---:|---:|---|---|---:|---:|
| 1 | 1759.9 | 1004.7 | 1.75× | 0.346/0.892/3.082 | 0.279/2.057/6.946 | 53.2 | 297.6 |
| 10 | 5385.3 | 5955.2 | 0.90× | 1.444/3.234/6.703 | 1.383/2.979/5.109 | 99.4 | 313.8 |
| 50 | 6343.5 | 5360.5 | 1.18× | 6.900/13.176/19.724 | 7.896/17.960/27.025 | 95.9 | 321.5 |

All three reference measurements pass with zero HTTP/semantic errors. The prepared forward path beats this reference at 1/50 VU but loses at 10 VU. Reference responses can contain richer metadata; ratios describe the shared tested contract, not identical full FHIR implementations. CPU uses interpolated process CPU-time deltas (100% = one core). RSS is the maximum one-second sample. PostgreSQL and load-client CPU/RSS are excluded from term-squid figures; server RSS is not total product memory.

The unexpected primary reverse-direction cliff was investigated after the primary matrix, without modifying the selected binary or overwriting any result. A separate TCP_NODELAY-only copy was built after those measurements; no compilation overlaps the follow-up probes. Short 15-second probes at 10 VU use 3-second warmup and counterbalance operation order (plain forward/reverse; TCP reverse/forward). These are diagnostics, not replacements for the 30-second scored runs.

| Listener | Direction | RPS | p50/p95/p99 ms |
|---|---|---:|---|
| plain | TS-CM01 | 4984.6 | 1.572/3.790/7.144 |
| plain | TS-CM02 | 4576.0 | 1.637/4.164/7.952 |
| tcp | TS-CM02 | 5084.9 | 1.521/3.474/6.722 |
| tcp | TS-CM01 | 5187.6 | 1.453/3.765/7.498 |

The same plain listener already improves to 4,576 reverse RPS on this fresh probe. The TCP differences are much smaller than the primary cliff and do not isolate its cause; TCP_NODELAY remains an unshipped experimental variant. A fresh run of the unchanged selected binary then repeats reverse translation with the exact primary 30-second/5-second protocol:

| VU | Repeat reverse RPS | p50/p95/p99 ms | CPU % | Sampled peak RSS MiB |
|---:|---:|---|---:|---:|
| 10 | 6256.7 | 1.315/2.400/5.282 | 60.4 | 13.5 |
| 50 | 6009.3 | 7.235/13.824/19.554 | 61.1 | 22.3 |

Both reverse repeats pass without HTTP/semantic errors and improve over the baseline primary points. The original 10/50-VU cliff is not consistently reproduced; its cause remains unresolved. The losing primary results remain the main comparison, and a stable tail-latency or universal performance claim is not justified by these laptop repetitions. Raw follow-ups are in `results/increment5-transport-*` and `results/increment5-reverse-repeat`.

## Remaining limits and next priority

The warm path still uses a generation query and SQLx return-to-pool health exchange per request. The pool policy from the previous increment is unchanged. Strong post-commit freshness must survive any future batching; a generation TTL would reintroduce stale positive/negative results and is not an acceptable shortcut.

Cold work still parses the full resource, and unrelated imports invalidate plans. Very large maps can bypass the cache budget; response volume/admission need stronger operational limits. The conservative conditional reverse-group error scope and the incomplete source/target version/parameter contract remain limitations. Selected synthetic passes do not establish clinical conformance. Subsumption's separate concept/closure reads remain another major engine bottleneck; the overall 3× reference and >12,000 simple-read goals are not established by this translation-only comparison.

## Evidence and reproduction

All raw evidence remains ignored under `.benchmarks/`: `profiles/increment5`, `results/increment5-*`, instrumented copies/binaries and `increment5-*.log`. Baseline server source is commit `839e14b`; selected release hashes and source archives are recorded in `profiles/increment5/sha256.txt`. The baseline binary is `backend-increment5-baseline` with SHA-256 `3acb7180c7c92295291faa543a6ff8b300f91308a794a255d9f7705e6e192ce1`. The exact updated harness is retained in `profiles/increment5/harness-source.tar.gz` (SHA-256 `92d09b8f634ca6db4b548627a21430ff2268c49c0997d2283a338c29802f9f22`). The final executable SHA-256 is `3717cae1b8234496dc7d6dcd657ebf64c53235c88b3153cd96906665c7ffdfa7`, and its source archive SHA-256 is `03ed0fb4158a5ef444b0d4d17c1965e094c9d3cd3daf7056a5ff77978e436a29`.

Reference source is based on `25a41f87711507789b361480ee4efae42df504d4`, Node 24.16.0. As in the previous report, an isolated copy pins only the startup R4 core package identifier to the cached `hl7.fhir.r4.core#4.0.1` to avoid a failing external registry lookup; query/worker code and default caching remain unchanged. The bootstrap patch is retained in `profiles/increment5/reference-bootstrap-pin.patch`. No public terminology service receives load. Sequential laptop runs remain descriptive, not randomized confidence estimates.

After applying migrations and starting the selected local server:

```sh
bun benchmarks/bench.ts run --tests TS-CM01,TS-CM02 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1'
```

For the local reference use `--tests TS-CM01 --name fhirsmith --url http://127.0.0.1:17003/tx/r4` with the same harness. Allocation probes use `PROFILE_OPERATION=translation`. CPU/RSS observation follows the benchmark README.
