# R4 correctness and performance checkpoint

November 6, 2026. Correctness fixes, final regression checks and both measurement phases are complete.

## Result

Concrete R4 translation defects are fixed; **414 tests pass, zero ignored**. Final requested allocation bytes fall by **1.56–2.58% for identity requests** and **83.94–86.47% for the separate gzip-capable client**. The gzip-capable path performs more allocation calls, not fewer. Neither diagnostic measures retained memory or CPU.

The final 50-VU generated finite cohort beats pinned FHIRsmith throughput by **2.07–5.47x**, but all four single-client points lose to the reference, and 11/12 primary throughput points lose to our original baseline. **Overall speed improvement and the clinical composite goal are not established.** The full 66 official windows retain **19,713,673 passing response checks, zero failures**; three folded diagnostics are counted separately.

## Scope

This checkpoint fixes concrete R4 ConceptMap interpretation and output defects without inventing clinical mappings. It preserves the six-operation API boundary, existing source/version selection, negative mapping semantics, bounded preparation and fresh generation-based reads. It does not establish full R4 conformance, clinical corpus completeness or an ecosystem/composite benchmark result.

Performance work is recorded separately below. Correctness changes are not evidence of a speed improvement, and passing local generated fixtures does not replace matched workloads against the pinned local FHIRsmith reference.

## Required mapping equivalence

R4 makes `ConceptMap.group.element.target.equivalence` a required modifier with a required terminology binding. Its absence cannot safely mean a positive relation. [R4 ConceptMap equivalence](https://hl7.org/fhir/R4/conceptmap-definitions.html#ConceptMap.group.element.target.equivalence)

`PreparedConceptMap::prepare` previously converted a missing or non-string value to `equivalent`. It now rejects missing, null, non-string, empty and unknown values. Every target is checked during preparation, including targets unrelated to the requested code; a later valid target cannot hide malformed input.

All ten R4 values remain accepted: `relatedto`, `equivalent`, `equal`, `wider`, `subsumes`, `narrower`, `specializes`, `inexact`, `unmatched` and `disjoint`. Explicit `unmatched`/`disjoint` targets may omit `target.code` and remain negative. They do not become a positive match or trigger an unmapped fallback for the explicitly described source code. Existing positive relation values and reverse relation handling are retained.

## Dependency and product namespaces

R4 defines the missing mapping-side namespace relative to the group: `dependsOn.system` uses the source system, while `product.system` uses the target system. [R4 dependency system defaults](https://hl7.org/fhir/R4/conceptmap-definitions.html#ConceptMap.group.element.target.dependsOn.system)

Preparation now resolves these effective systems. An explicit criterion system wins; an omitted one uses the applicable group system. A missing/unusable default or mistyped explicit system fails explicitly rather than becoming a wildcard. Thus a dependency with matching code in `urn:evil` cannot satisfy an unqualified criterion belonging to `urn:s`.

Request-side dependency `Coding.system` remains optional. The bounded server policy is to interpret an omitted request system in the actual translation input's namespace, not in an arbitrary external criterion namespace. This is an explicit context policy, not a claimed universal R4 default for anonymous Coding values. Reverse translation uses its actual reversed input system. Conditions and products retain their original namespaces when their roles reverse.

## R4 product output and explicit versions

R4 input `dependency.concept` is a `CodeableConcept`, but output `match.product.concept` is a `Coding`. The response now uses `valueCoding`; the input contract is unchanged. [R4 translate operation](https://hl7.org/fhir/R4/conceptmap-operation-translate.html)

Mapping criterion systems are canonical references and can include an explicit business version. Effective product canonicals are now split into `TranslationProduct.system` and `TranslationProduct.version`, then emitted as `Coding.system` and `Coding.version`. For example, `urn:external|v1` yields system `urn:external` and version `v1`, never a pipe-suffixed Coding system. The same rule applies when a versioned dependency becomes a reverse output product. [R4 canonical references](https://hl7.org/fhir/R4/datatypes.html#canonical), [R4 Coding system/version](https://hl7.org/fhir/R4/datatypes-definitions.html#Coding.system)

Explicit unversioned systems remain unversioned. Group `sourceVersion`/`targetVersion` values are not silently inferred for products. The principal translated Coding's existing system/version behavior is unchanged.

## Admission, publication and stored reads

- **Core:** all adapters share `PreparedConceptMap::prepare`; the unsafe equivalence default and wildcard namespace interpretation are removed there.
- **Shared JSON admission:** both protected R4 inspection and CLI inspection validate ConceptMaps. `import_resource` validates again before identity admission or writes, so a retained `Inspection` cannot authorize a subsequently changed unsafe payload.
- **Protected FHIR packages:** resource inspection rejects malformed maps; publication revalidation also catches payload changes after inspection. A late mapping failure rolls back earlier resource writes and generation changes in the caller-owned transaction.
- **CLI FHIR packages:** `commands/import.rs::import_concept_map` validates before its first publication SQL. The generic package parser is not made into a new schema validator. The real public CLI import path is tested with a valid early CodeSystem and a malformed later ConceptMap, followed by a valid package control.
- **Inline and request-local maps:** malformed modifiers fail before successful translation, including requests for unrelated codes. No map is repaired by assuming `equivalent`.
- **Stored maps:** new requests fail closed after a committed malformed direct SQL update, with retained plans enabled or disabled. The checks cover URL/instance reads and contextual map discovery. Uncommitted updates do not change another reader's snapshot; repaired content is observed without a TTL or restart.

No database schema migration or automatic clinical-data rewrite is introduced by these fixes. Previously tolerated malformed maps can remain stored, but readers reject them. Correct their authored source and normal publication identity rather than assigning a guessed equivalence. Existing declared `unmapped.mode` behavior is outside this correction; no new unmapped default is added.

## Fixture repairs

Six older positive test fixtures depended on the removed implicit `equivalent` value. They now state their intended equivalence explicitly; eight target entries were repaired. Assertions were not relaxed:

- `crates/core/src/translation.rs`: `unmapped_rules_never_override_explicit_negative_or_conditional_entries`, `fixed_and_referenced_unmapped_modes_preserve_explicit_matches_and_continuations`, `indexes_preserve_order_duplicates_reverse_display_and_equivalence`, and `conditional_error_scope_and_unmapped_modes_are_explicit`.
- `crates/backend/src/tests.rs`: `prepared_translation_versions_updates_and_concurrency`.
- `crates/backend/src/api_completion_tests.rs`: `translate_discovery_scope_filtering_chains_and_fixed_reverse_share_real_content`.

The existing product-output assertion now checks `valueCoding` rather than `valueCodeableConcept`. Original ecosystem goldens, benchmark scoring and frozen workload inputs are not changed by this correction.

## Targeted correctness evidence

The final targeted workspace translation run passed **23 tests, zero ignored**: ten backend tests, one CLI package test, eleven core tests and one ingestion inspection test. This is a targeted count, not a full-workspace total. It covers required modifier types and values, code-less negative mappings, group defaults and explicit namespaces, forward/reverse product Coding and versions, malformed inline/request-local/stored maps, committed/rolled-back updates, retained-cache modes and actual CLI package rollback.

Additional ingestion JSON/package admission and changed-payload publication tests were run separately. Workspace Clippy with warnings denied also passed. Raw evidence is kept under the ignored `.benchmarks/spec-speed-20261106/` directory:

- `translation-product-targeted.log`, `translation-product-clippy.log`;
- `translation-ingestion-all.log`, `translation-existing-api.log`;
- `translation-cli-public-entry.log`.

New regressions are in `crates/backend/src/translation_safety_tests.rs`, the core translation tests, ingestion inspection/package tests and `crates/cli/src/commands/import_translation_tests.rs`.

## Remaining comparison architecture gaps

These corrections do not change code identity or case policy:

- **Unicode comparison keys:** the recorded PostgreSQL/Rust boundary remains. Stored lookup uses database case mapping, while the independent finite/request-local engine uses Rust Unicode lowercasing. The existing `U+0130`/`i`/`i` plus combining-dot reproduction therefore remains a consistency limitation, not a translation fix or timing result. See [the exact reproduction and preserved responses](REQUEST_CONTEXT_REPORT.md#remaining-unicode-comparison-boundary).
- **Missing case policy:** `PreparedSystem` and the generated SQL `case_sensitive` projection currently collapse absent `CodeSystem.caseSensitive` to `true`. R4 instead leaves that policy unspecified and advises tolerant input when it is unknown. This default-case gap remains open; it is distinct from an explicitly case-insensitive system's Unicode mismatch. [R4 caseSensitive semantics](https://hl7.org/fhir/R4/codesystem-definitions.html#CodeSystem.caseSensitive)

A shared comparison policy needs an explicit design covering absent/true/false metadata, ambiguity/collisions, canonical source spelling, ingestion and persisted projections, SQL fallback, finite/native composition, request-local definitions, retained plans and the reusable engine. Any projection migration or rebuild must be specified before changing identity semantics. Neither a local database locale nor a passing ASCII case suite resolves that architecture decision.

## Read-cost implementation and allocation evidence

## Exact hierarchy snapshot hint

`HierarchyPlans::resolve` retains a weak hint only for a prepared, case-sensitive hierarchy snapshot. It captures the cache generation and `Weak<Snapshot>` before awaiting a fresh `GenerationReads::current(Family::Hierarchy)` database observation. It uses that snapshot only when the observed generation matches and the weak reference can still be upgraded.

The hint does not replace the database observation, extend the snapshot lifetime, or retain a cache lock across SQL. Exact comparisons need no database-folded probe keys, so this path avoids requesting and allocating those keys. Folded, SQL, and unsupported snapshots retain the keyed observation path, with an extra initial hint/cache inspection. Their cost is checked separately rather than called unchanged.

A changed generation or an expired weak reference falls back to normal cache resolution and cold preparation. An exact hint cannot resolve a concurrently installed folded snapshot using empty keys: that case enters the existing read-only repeatable-read transaction and obtains comparison keys from the same database snapshot. Cache admission, eviction, preparation bounds, canonical/version selection, and SQL fallbacks are unchanged.

## Bounded generation batching

`GenerationReads` reuses two `VecDeque` buffers instead of discarding the detached queue allocation after every observation. The pending queue remains limited to 256 entries. A detached active batch contains at most another 256 entries, so an instance retains at most 256 active plus 256 pending request records; buffer reuse does not introduce an unbounded request queue or a persistent worker.

The runner acquires a database connection before detaching the pending batch. That detach point remains the observation boundary: requests admitted after detachment cannot join an already dispatched observation and must receive a later database observation. The active batch is drained with `pop_front`, then its empty buffer is returned for reuse. No completed observation is cached for a later request.

Cancelled pending waiters are removed, and a full queue still returns the existing overload error. Dropping an HTTP future does not cancel an active shared SQL observation. SQL failures are delivered to the detached waiters, and runner cleanup resets the running flag and closes pending replies rather than leaving a stuck queue.

## Isolated gzip allocation owner

The initial warm profiles requested roughly 371–382 thousand allocation bytes per request across all four operations. Future/frame diagnostics did not account for this amount: the generation runner frame was 8,272 bytes and the larger store/provider futures were approximately 8–9 KiB each.

A database-free, in-process comparison isolated the locked gzip stack by sending the same 288-byte FHIR-shaped JSON through the original compression middleware:

| Encoding | Allocation calls/response | Requested bytes/response | Body bytes |
| --- | ---: | ---: | ---: |
| Identity | 24 | 3,055 | 288 |
| Gzip | 31 | 326,510 | 132 |
| Gzip increment | 7 | 323,455 | — |

The gzip output decoded byte-for-byte to the original payload. This comparison used no database or network and did not measure throughput or CPU.

The enabled dependency chain was `tower-http 0.7.1` → `compression-codecs 0.4.47` → `flate2 1.1.10` with `rust_backend` → `miniz_oxide 0.9.1`. The original default predicate compressed eligible bodies of at least 32 bytes. Per-response large allocations attributable to compressor state were:

| Allocation owner | Size, bytes | Count |
| --- | ---: | ---: |
| Compressor state | 65,712 | 1 |
| Output buffer | 85,196 | 1 |
| Dictionary | 33,026 | 1 |
| Hash/next buffers | 65,536 | 2 |
| Total listed state | 315,006 | 5 |

These five allocations explain most, not all, of the isolated 323,455-byte gzip increment. The remainder includes additional codec and buffering allocations. Requested bytes describe allocation traffic, not live memory, peak RSS, or bytes retained after a request.

Evidence under `.benchmarks/spec-speed-20261106/`: `compression-owner-diagnostic.json`, `compression-histogram.log`, `compression-equality.json`, `compression-feature-tree.txt`, and `frame-diagnostic.log`. Diagnostic binaries are immutable copies under `bin/`; they are not the primary timing binaries.

## Transport policy

The candidate leaves eligible, known bodies smaller than 1,024 bytes uncompressed when identity is acceptable. Eligible larger bodies and unknown-size streams retain gzip support. It does not buffer or poll a body to decide its size, and it preserves the existing image, SSE, gRPC, range, and preencoded-response exclusions.

Explicit refusal of identity overrides the small-body threshold when gzip is acceptable and the representation is eligible. If no acceptable representation can be sent, the middleware returns an empty 406 response instead of forwarding forbidden identity or an unacceptable preencoded body. It checks all supplied `Content-Encoding` values, recognizes the `x-gzip` alias, and preserves existing `Vary` tokens while adding `Accept-Encoding` when needed.

Informational, 204, and 304 responses bypass the compression service before its negotiation can overwrite their status. HEAD also bypasses compressor construction. It preserves explicit HEAD handlers, never substitutes GET, and does not poll a response stream. Known identity/preencoded representation lengths are retained; encoded lengths that are not known are omitted rather than replaced with a false zero. An unknown-size empty wrapper prevents axum from inventing `Content-Length: 0` before its final HEAD stripping.

[HTTP HEAD and content negotiation](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.3.2) remain response semantics, not a benchmark-only path.

### Final no-header fast path

The final `TransportLayer` calls its inner typed service directly when `Accept-Encoding` is absent. That future only preserves/adds `Vary`; it does not enter `from_fn`, clone negotiation metadata or construct `CompressionBody`. Normal Axum GET/HEAD routing, body/status handling and selected lengths remain intact. This is not a bypass of terminology selection, fresh SQL generations or response checks.

Header-present requests retain the separately tested negotiated middleware. Its boxed service is built once during layer construction and shared through `Arc`, rather than boxed on every ordinary no-header clone. The slow request releases any unused inner readiness reservation before polling its independent service clone. Tests prove the typed future branch, shared slow-service ownership, no-header GET/HEAD/preencoded/bodyless behavior and all prior negotiation cases. Actual final identity/gzip profiles below supersede assumptions about this branch.

The transport change avoids constructing the isolated gzip state for ordinary small responses. It does not establish a CPU or throughput improvement by itself, and it deliberately permits increased identity wire bytes for those small responses. Large-response gzip and explicit client negotiation remain part of the workload rather than being disabled to improve a score.

## Final allocation profiles: keep negotiation separate

## Workload and controls

- Unchanged `benchmarks/local/cases.js`: TS-LK01, TS-SS01, TS-VC01 and TS-CM01; unchanged `accepts` checks.
- Each profile performs 40 warmup requests then 1,000 strictly checked measured requests per operation: 160 warmup + 4,000 measured requests. Probe endpoints are outside those semantic request counts; probe/runtime/connection overhead is not subtracted from the process counter differences.
- Identity runs use pinned Node v24.16.0 `node:http`, `Accept: application/fhir+json`, no `Accept-Encoding`, and no automatic decompression. For each of the three variants, all **4,168 serialized requests** (4,160 semantic requests plus eight probes) were checked for header absence; all responses had absent/identity coding.
- Default-client runs use the original unchanged Bun `profile.ts`. Their `final-gzip` label denotes the default gzip-capable client, not actual compression of every final response. Keep these comparisons separate from the Node identity arm.
- All profiles use the same original allocation probe. The recreated owned database loads the same five immutable generated resources and all migrations; old database/profile provenance remains intact. Physical database identity differs from the original Bun baseline, so these are allocation diagnostics, not a timing comparison.

## Identity: all three variants

Values are allocation calls / requested bytes per measured request. Requested bytes are not live-memory bytes.

| Operation | Baseline calls | Baseline bytes | Intermediate calls | Intermediate bytes | Final calls | Final bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TS-LK01 | 149.137 | 51,792.729 | 157.057 | 53,171.382 | 148.048 | 50,872.734 |
| TS-SS01 | 133.056 | 47,445.764 | 130.138 | 48,463.270 | 121.141 | 46,221.999 |
| TS-VC01 | 134.846 | 58,180.574 | 142.853 | 59,582.358 | 133.842 | 57,275.478 |
| TS-CM01 | 157.646 | 52,759.384 | 165.645 | 54,128.240 | 156.644 | 51,862.520 |

### Final identity changes

| Operation | Calls vs baseline | Requested bytes vs baseline | Calls vs intermediate | Requested bytes vs intermediate |
| --- | ---: | ---: | ---: | ---: |
| TS-LK01 | -0.730% | -1.776% | -5.736% | -4.323% |
| TS-SS01 | -8.955% | -2.579% | -6.913% | -4.625% |
| TS-VC01 | -0.745% | -1.556% | -6.308% | -3.872% |
| TS-CM01 | -0.636% | -1.700% | -5.434% | -4.186% |

All four identity cases request fewer allocations and fewer bytes in the final variant than in either earlier variant. The requested-byte reduction against baseline is about **1.56–2.58%**, not the roughly 85% reduction visible in the separate default-client comparison. No CPU/RPS or retained-memory inference follows from either table.

## Original default Bun client: baseline versus final

| Operation | Baseline calls/request | Final calls/request | Calls change | Baseline requested bytes/request | Final requested bytes/request | Requested bytes change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TS-LK01 | 156.142 | 171.146 | +9.609% | 375,570.081 | 54,967.366 | -85.364% |
| TS-SS01 | 140.065 | 144.063 | +2.854% | 371,224.304 | 50,245.300 | -86.465% |
| TS-VC01 | 141.852 | 156.852 | +10.574% | 381,949.818 | 61,338.046 | -83.941% |
| TS-CM01 | 164.652 | 179.654 | +9.111% | 376,528.628 | 55,925.088 | -85.147% |

The default-client requested-byte reduction is **83.94–86.47%**, but allocation calls **increase in every operation**, by approximately 2.85–10.57%. Across this fixed equal-sized four-case diagnostic, calls are 602,711 → 651,715 (+8.131%), while requested bytes are 1,505,272,831 → 222,475,800 (-85.220%). This aggregate is not a production request mix.

### Actual encoding evidence

- `baseline-gzip-encoding-evidence.json`: Content-Encoding = `gzip`, HTTP 200, automatically decoded body satisfies the original strict TS-LK01 case, instrumentation binary SHA-256 identical before/after.
- `final-gzip-encoding-evidence.json`: Content-Encoding = **absent**, HTTP 200, automatically decoded body satisfies the original strict TS-LK01 case, instrumentation binary SHA-256 identical before/after.

The supplemental baseline request demonstrates actual gzip negotiation by default Bun fetch. The final small lookup response correctly has no Content-Encoding under the preserved 1 KiB threshold. These two requests are not included in profile or timing totals; their headers attest that lookup example, not every Bun response. The initial supplemental diagnostic incorrectly assumed the final response must be gzip; its failed assumption logs are retained separately as a diagnostic setup error, **not** a benchmark/profile failure. No raw allocation profile was edited.

The original intermediate Bun profile remains in `candidate-allocations.json`; it is not attributed to the final typed service. All evidence paths above are relative to `.benchmarks/spec-speed-20261106/`.

## Matched optimized workload

This is a generated finite workload, not a clinical composite or a fresh ecosystem audit. The unchanged local runner, requests, pools, checks and client encoding headers are retained. Only `TS-LK01`, `TS-SS01`, `TS-VC01` and `TS-CM01` are timed, at 1/10/50 VU for 30 seconds each, with five seconds of per-operation warmup. Builds, tests and allocation/frame/compression profiles finish before timing; participants run sequentially on the same host.

### Artifacts and environment

- Apple M3 Pro, ARM64, macOS 26.7.1; Rust/Cargo 1.99.1, Bun 1.4.4, k6 2.3.0. These are not the older Apple M1 measurements.
- All three Rust binaries use release optimization, PostgreSQL 17 on loopback, pool 32 and prepared retention enabled, on an owned temporary database per measurement phase. The original database was dropped only after its cohort; the final phase uses a new owned database loaded with the identical six-file package inventory. Existing benchmark databases are not replaced. No admin/session work is added to public reads.
- Baseline: preserved uncommitted 379-test checkpoint, 406 frozen working-tree files; binary SHA-256 `7824007c77d6839677d638363aa21d71f0979467b41ec7576ca4c460acbeaabd`.
- Intermediate candidate (dynamic middleware): 414 frozen working-tree files; binary SHA-256 `88c538abbc20fc28df788cd54271f5fdc6731168de9c8c7945e8b3a4843f07de`. This remains an intermediate artifact, not the final implementation.
- Final typed candidate: 414 frozen working-tree files; binary SHA-256 `dedb55cec3896a51a36b3de1e3f93fa82fe1674755b92b56e3c6c7dcd935c909`. Current crate sources match this frozen build. All normal timed artifacts are uninstrumented.
- FHIRsmith 0.18.2, revision `25a41f87711507789b361480ee4efae42df504d4`, Node 24.16.0, owned loopback endpoint `/tx/r4`. The preserved bootstrap patch binds loopback and pins mandatory R4 core to 4.0.1; it does not modify translation/subsumption or the matcher. Patch SHA-256 `cb1d26cfb3dc1a0078b74a91cfff106713e99f45bc8110777f4f633d97e0afed`.
- All raw evidence, manifests, source archives, immutable normal/diagnostic binaries, configuration and runners are under `.benchmarks/spec-speed-20261106/`; unchanged runner outputs are under `.benchmarks/results/spec-speed-*`.

### Actual matched content and gates

Both servers load the exact same five-resource benchmark package: three CodeSystems, one ValueSet and one ConceptMap. FHIRsmith additionally has its normal mandatory R4-core bootstrap cache; identical selected content is not a claim of identical total server inventory. The synthetic package contains no clinical release.

The runner's four-resource fingerprint is `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`. Its fixture directory also retained a prior generated `CodeSystem-benchmark-hierarchy-folded.json` (10,001 concepts), which fixture generation does not clear. Both participants receive an immutable copy of the complete actual package. The separate six-file manifest, including `package.json`, has SHA-256 `c103811ee7e547c529134dbdb27e3fb229ef258fd294edc1c335049273ac4e38`; it explicitly records the extra file rather than pretending the runner hash covers it. Benchmark sources and assertions are unchanged.

Baseline and candidate pass **7/7 unchanged local gates**. Pinned FHIRsmith passes **6/7**: the reverse `TS-CM02` gate returns the original target in `concept` and source in `sourceConcept`, failing the required reversed Coding. That failure is retained, not patched or waived. All four selected timed scenarios pass each participant's unchanged preflight. Their timing results do not cover the failed reverse scenario or prove general equivalence.

### Final primary results

Each cell is requests/second followed by p95 milliseconds; ratios above 1 favor final throughput. Baseline and reference columns are the original phase's unchanged matrices, not newly repeated full matrices. The final phase uses the identical generated content on a recreated owned database; fresh reverse-order subsumption controls below test all three participants again. Do not treat the later final cohort as a fully counterbalanced experiment.

| Operation | VU | Baseline req/s; p95 ms | FHIRsmith req/s; p95 ms | Final req/s; p95 ms | Final / baseline | Final / FHIRsmith |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TS-LK01 | 1 | 1,788; 0.562 | 4,197; 0.207 | 1,803; 0.587 | 1.01x | 0.43x |
| TS-LK01 | 10 | 10,043; 1.150 | 9,550; 1.261 | 9,457; 1.280 | 0.94x | 0.99x |
| TS-LK01 | 50 | 24,164; 3.054 | 9,467; 8.220 | 21,502; 3.771 | 0.89x | 2.27x |
| TS-SS01 | 1 | 1,801; 0.538 | 4,709; 0.185 | 1,756; 0.583 | 0.98x | 0.37x |
| TS-SS01 | 10 | 9,907; 1.139 | 10,906; 1.135 | 9,255; 1.330 | 0.93x | 0.85x |
| TS-SS01 | 50 | 24,029; 3.196 | 10,902; 7.135 | 22,528; 3.489 | 0.94x | 2.07x |
| TS-VC01 | 1 | 1,825; 0.550 | 2,352; 0.441 | 1,777; 0.590 | 0.97x | 0.76x |
| TS-VC01 | 10 | 10,410; 1.084 | 4,284; 2.979 | 9,355; 1.365 | 0.90x | 2.18x |
| TS-VC01 | 50 | 25,108; 2.863 | 4,224; 17.972 | 23,118; 3.269 | 0.92x | 5.47x |
| TS-CM01 | 1 | 1,823; 0.537 | 4,008; 0.224 | 1,751; 0.589 | 0.96x | 0.44x |
| TS-CM01 | 10 | 9,772; 1.320 | 8,377; 1.458 | 9,112; 1.455 | 0.93x | 1.09x |
| TS-CM01 | 50 | 22,939; 3.350 | 8,049; 9.439 | 22,647; 3.322 | 0.99x | 2.81x |

- At 50 VU, final throughput is **2.07–5.47x** the pinned reference across these four finite operations, with lower p95 in each.
- At 1 VU, final throughput is only **0.37–0.76x** the reference, with worse p95 in each. Low-concurrency leadership is not achieved.
- At 10 VU, final beats the reference for ValueSet validation and translation, but not lookup or subsumption.
- Against our original baseline, final throughput is lower in **11/12** points; lookup at 1 VU is slightly higher while its p95 is worse. The largest primary throughput loss is lookup at 50 VU, about 11%. Smaller requested allocation traffic is not evidence of a throughput win.
- Final usually recovers throughput against the dynamic intermediate, but not every point improves and this does not establish stable superiority over baseline. The underlying cause of the remaining throughput/tail differences is not isolated.

The full four-artifact primary dataset contains **48 windows / 14,651,304 passing semantic response checks**, zero failed. No measured point is discarded. Final reverse controls and the complete intermediate matrix remain below.

### All final primary regressions

Listed metrics are worse than the comparator in that raw window: lower throughput or higher p50/average/p95/p99. `none` means only that no raw regression occurs in these five metrics. Exact equality is not listed. The JSON retains every exact ratio and percentage.

| Operation | VU | Against original baseline | Against original FHIRsmith | Against intermediate |
| --- | ---: | --- | --- | --- |
| TS-LK01 | 1 | p95 | throughput, p50, avg, p95, p99 | throughput, avg, p95, p99 |
| TS-LK01 | 10 | throughput, p50, avg, p95, p99 | throughput, avg, p95, p99 | none |
| TS-LK01 | 50 | throughput, p50, avg, p95, p99 | none | none |
| TS-SS01 | 1 | throughput, avg, p95, p99 | throughput, p50, avg, p95, p99 | none |
| TS-SS01 | 10 | throughput, p50, avg, p95, p99 | throughput, p50, avg, p95, p99 | p99 |
| TS-SS01 | 50 | throughput, p50, avg, p95, p99 | none | p99 |
| TS-VC01 | 1 | throughput, p50, avg, p95, p99 | throughput, p50, avg, p95 | throughput, p95 |
| TS-VC01 | 10 | throughput, p50, avg, p95, p99 | none | p99 |
| TS-VC01 | 50 | throughput, p50, avg, p95, p99 | none | p95 |
| TS-CM01 | 1 | throughput, p50, avg, p95, p99 | throughput, p50, avg, p95, p99 | none |
| TS-CM01 | 10 | throughput, p50, avg, p95 | p99 | none |
| TS-CM01 | 50 | throughput, p50, avg | none | p50 |

Final throughput remains below original baseline in **11 of 12** primary points. Compared with intermediate, final throughput rises in ten points but falls in lookup/1 VU and validation/1 VU. Some final tail latencies still regress against intermediate even where rate improves. These cross-phase differences do not establish causal or statistical gains.


### Intermediate primary results

The first candidate completed 36 matched primary windows and nine reverse-control windows with **13,257,207 passing semantic response checks, zero failed**. It did not establish a throughput improvement over baseline. These results are retained while a typed no-header fast path is implemented and measured as a new artifact.

The actual k6 request sends `Accept: application/fhir+json` but **no `Accept-Encoding` header**, as demonstrated by one request to an owned loopback capture receiver. The locked compression layer selects identity for this request. The Bun gzip allocation profiles therefore cannot explain or be presented as allocation savings on the primary k6 path. Raw request, k6 version/options/source/binary hashes and the one-request summary are in `k6-default-encoding-capture/`.

The initial candidate used dynamic `from_fn` and an additional compression/body wrapper even for identity requests. The final revision separates that ordinary no-header path with a typed service, while retaining the fully negotiated path and tests. Its actual allocation and throughput effects are reported separately, not assumed.

#### Retained intermediate matrix

Here “Candidate” means the superseded dynamic-middleware artifact, not the final typed build. All twelve points remain visible:

| Operation | VU | Baseline req/s; p95 ms | FHIRsmith req/s; p95 ms | Candidate req/s; p95 ms | Candidate / baseline | Candidate / FHIRsmith |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TS-LK01 | 1 | 1,788; 0.562 | 4,197; 0.207 | 1,826; 0.557 | 1.02x | 0.44x |
| TS-LK01 | 10 | 10,043; 1.150 | 9,550; 1.261 | 8,782; 1.606 | 0.87x | 0.92x |
| TS-LK01 | 50 | 24,164; 3.054 | 9,467; 8.220 | 19,893; 4.511 | 0.82x | 2.10x |
| TS-SS01 | 1 | 1,801; 0.538 | 4,709; 0.185 | 1,707; 0.660 | 0.95x | 0.36x |
| TS-SS01 | 10 | 9,907; 1.139 | 10,906; 1.135 | 8,882; 1.547 | 0.90x | 0.81x |
| TS-SS01 | 50 | 24,029; 3.196 | 10,902; 7.135 | 22,175; 3.524 | 0.92x | 2.03x |
| TS-VC01 | 1 | 1,825; 0.550 | 2,352; 0.441 | 1,779; 0.582 | 0.97x | 0.76x |
| TS-VC01 | 10 | 10,410; 1.084 | 4,284; 2.979 | 9,208; 1.417 | 0.88x | 2.15x |
| TS-VC01 | 50 | 25,108; 2.863 | 4,224; 17.972 | 23,054; 3.268 | 0.92x | 5.46x |
| TS-CM01 | 1 | 1,823; 0.537 | 4,008; 0.224 | 1,554; 0.886 | 0.85x | 0.39x |
| TS-CM01 | 10 | 9,772; 1.320 | 8,377; 1.458 | 8,217; 2.110 | 0.84x | 0.98x |
| TS-CM01 | 50 | 22,939; 3.350 | 8,049; 9.439 | 22,577; 3.346 | 0.98x | 2.80x |

### Reverse and folded controls

The first nine-window reverse matrix passed 2,627,976 response checks. Its intermediate/baseline single-client direction flips versus the primary order; at 50 VU the intermediate remains 11.94% below baseline, with p95/p99 28.09%/40.47% worse. Keep `control-analysis.json` and `control-summary.md` for every earlier percentile, order variation and regression.

#### Second reverse-order exact hierarchy

Participant order: final → baseline → fresh-process FHIRsmith; each retained the fixed 1, 10, then 50 VU matrix. The term-squid baseline and final windows use the new owned database. FHIRsmith uses its own freshly restarted process with the same immutable fixture inputs, not that PostgreSQL database.

| VU | Baseline req/s; p95; p99 ms | FHIRsmith req/s; p95; p99 ms | Final req/s; p95; p99 ms | Final / baseline | Final / FHIRsmith |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1,690.48; 0.666; 1.174 | 4,371.77; 0.198; 0.791 | 1,719.77; 0.687; 1.173 | 1.0173x | 0.3934x |
| 10 | 8,705.27; 1.477; 3.686 | 9,330.19; 2.157; 4.034 | 9,320.34; 1.361; 3.451 | 1.0707x | 0.9989x |
| 50 | 16,183.18; 6.185; 12.247 | 7,600.32; 13.880; 28.817 | 22,214.29; 3.519; 6.008 | 1.3727x | 2.9228x |

##### Every second-reverse regression

- 1 VU against baseline: p95 +3.153%.
- 1 VU against fhirsmith: throughput -60.662%, p50 +202.454%, avg +185.361%, p95 +246.970%, p99 +48.302%.
- 10 VU against baseline: none in throughput/p50/average/p95/p99.
- 10 VU against fhirsmith: throughput -0.106%, p50 +14.356%, avg +0.242%.
- 50 VU against baseline: none in throughput/p50/average/p95/p99.
- 50 VU against fhirsmith: none in throughput/p50/average/p95/p99.

##### Order and phase variability

| VU | Baseline reverse2 vs original primary | FHIRsmith reverse2 vs original primary | Final reverse2 vs final primary | Final / baseline original-primary → reverse2 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | -6.14% | -7.17% | -2.09% | 0.9753x → 1.0173x |
| 10 | -12.13% | -14.45% | +0.71% | 0.9342x → 1.0707x |
| 50 | -32.65% | -30.28% | -1.39% | 0.9376x → 1.3727x |

At 50 VU, the reverse2 baseline and reference rates fall markedly versus their original primary values while final stays much closer to its own primary rate. Thus the large reverse2 relative advantage is not a clean code-effect estimate. The baseline-relative throughput sign changes from final primary losses at all three SS points to reverse2 gains; reference-relative behavior also varies. Earlier intermediate reverse controls remain in `control-summary.md`, including their 50-VU baseline loss and worse tails. Keep both control rounds; neither supersedes a primary regression.

#### Three folded/keyed controls

Each raw global summary combines 5 seconds warmup and a 30-second one-VU scenario. There is no separate steady-state subgroup, no p99 summary and no folded FHIRsmith run. The final run belongs to the later database/host phase.

| Participant | HTTP requests | Check passes | Global req/s | Median ms | Average ms | p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline | 63,683 | 127,366 | 1,819.486 | 0.495 | 0.508338 | 0.55700 |
| intermediate | 63,653 | 127,306 | 1,818.619 | 0.496 | 0.508457 | 0.56000 |
| final | 55,638 | 111,276 | 1,589.649 | 0.507 | 0.576363 | 0.99515 |

Every compared final folded metric is worse in these raw summaries:

- Against baseline: throughput -12.632%, p50 +2.424%, avg +13.382%, p90 +37.288%, p95 +78.662%.
- Against intermediate: throughput -12.590%, p50 +2.218%, avg +13.355%, p90 +36.773%, p95 +77.705%.

The final folded rate is lower by roughly 12.6% and p95 is markedly higher; do not claim a harmless or improved keyed fallback from the identity allocation reductions. Both old and final folded runs emit high-cardinality client-metrics warnings:

- baseline: 100,490, 201,074, 400,577 unique time series (three warnings).
- intermediate: 101,507, 201,308, 400,379 unique time series (three warnings).
- final: 100,229, 201,605, 400,001 unique time series (three warnings).

Client metrics-engine pressure, order and host/database/process state remain possible confounders; no specific cause or statistical significance is established. These measurements are not peak/product-capacity or clinical claims.

The two reverse rounds total **18 windows / 5,062,369 passing response checks**. Together with the 48 primary windows, this is **66 windows / 19,713,673 checks**, zero failed. The three folded summaries are separate: **182,974 requests / 365,948 checks**. Exact regression inventories and provenance remain in `control-analysis-final.json` and `control-summary-final.md`.

## Final verification

- Full workspace: **414 passed, zero failed or ignored** — backend 254, CLI 60, core 86, ingestion 14.
- `cargo check --locked --offline --workspace`, workspace Clippy with all targets/features and warnings denied, formatting and diff whitespace checks pass.
- Independent `tests/core-consumer` builds and runs. The reusable core remains independent of PostgreSQL, HTTP and the backend.
- Unchanged harness: **38 tests, 40,251 assertions**; ecosystem adapter unit tests: **4 passed**.
- New transport regressions: **13 passed**, including threshold edges, exact decoded bytes, excluded MIME, streams, identity/wildcard refusal, repeated headers, bodyless statuses, preencoded output and explicit/implicit HEAD with no stream polling.
- The first workspace attempt failed an existing snapshot fixture: edition 2's database timestamp preceded edition 1 by 99.292 ms. The production selector correctly chose edition 1. The fixture now assigns explicit ordered activation timestamps inside the publication transactions and asserts the selected edition before releasing its gate; original result/display/version assertions are unchanged. Twelve fixed repetitions, all thirteen finite-semantics tests and the final full workspace pass. Evidence: `display-snapshot-failure-evidence.txt`, `workspace-initial.log`, `workspace-final.log` and final `workspace-typed-final.log`/`clippy-typed-final.log`.

Optimized API artifacts reuse the unchanged tracked embedded assets; no fresh frontend build or browser delivery is claimed. No schema migration, new dependency or lockfile change is introduced. The already resolved `tower` utility feature is now explicit. Earlier migrations through `20261106000004_snomed_simple_refsets` remain required. Prior working-tree changes are preserved. After validation, the user requested one commit collecting the accumulated delivery. Pushing and deployment remain separate.

## Cleanup and next work

Both comparison databases and the owned reference processes were removed/stopped after their phases; the failed test's owned schema was removed after saving its evidence. The existing PostgreSQL instance, benchmark database and unrelated processes remain. `cleanup.json` and `cleanup-final.json` record exact identities.

The next performance task is an isolated attribution of remaining throughput/tail cost, not another unsupported speed claim: distinguish typed transport/future layout, generation queue handoff and client/order effects using fixed controls. Do not assume TCP_NODELAY is a fix: prior transport/translation reports did not establish an independent gain. Keep the shared Unicode/unknown-case policy design separate, with migration/collision requirements before implementation. Actual clinical content and unchanged-composite admission remain necessary for the product goal. This checkpoint does not rerun the historical general-mode ecosystem audit or establish full R4 conformance.
