# Prepared hierarchy plans — November 3, 2026

At 50 VU, the original exact-case workload improves from 5,498 to 6,313 RPS; a reverse-order repeat narrows the gain to 6,029 → 6,260 RPS. The separate 10,001-concept folded workload improves from 1,285 to 7,289 RPS. FHIRsmith still wins the original workload, and fails folded preflight, so neither the overall 3x reference nor >12,000 simple-read target is achieved.

## Implementation

`term-squid-core::PreparedHierarchy` indexes stored codes and materialized transitive relationships. A subsumption request resolves both concepts and tests sorted ancestor IDs without decoding FHIR JSON or reading the closure again. This is an immutable hierarchy plan, not a per-code response cache or a graph traversal on every request.

The PostgreSQL adapter reads metadata, concept keys and the existing closure projection in one repeatable-read, read-only transaction. It does not rebuild the hierarchy from resource JSON: fragment/legacy projections keep the same semantics as the atomic SQL implementation. Versioned canonicals, default-release ordering and instance UUID selection remain unchanged. Missing codes, equivalence, both strict directions, unrelated codes and unsupported-hierarchy error precedence are preserved.

Comparison keys come from PostgreSQL `lower()` on both preparation and query paths. Rust Unicode lowercasing is not substituted for the database policy. The library accepts provider-defined keys and returns stored spelling. Duplicate/ambiguous keys and unknown closure endpoints fail explicitly rather than publishing a partially valid plan.

## Freshness and admission

Every warm request executes a fresh generation query, including the two database-folded query keys, before consulting the cache. That statement is its snapshot boundary: an operation already observing an old generation may finish against it, but a request starting after a committed write cannot reuse it. There is no TTL, delayed invalidation, per-code answer retention or stale fallback on SQL failure. This remains a database-dependent hot path, not the database-free goal in Milestone 2.

Migration **`20261103000000_hierarchy_generation` must be applied before starting this build**. It adds `hierarchies_tracked` and statement-level transactional generation triggers for INSERT/UPDATE/DELETE/TRUNCATE on `concepts` and `closure_table`. Resource triggers alone cannot invalidate projection-only updates. Schema-guard/query errors fail closed. Existing CodeSystem/ValueSet/ConceptMap triggers remain; an unrelated write can still invalidate every prepared family. Database owners must preserve triggers and commit resource/projection changes together.

Warm reads bypass cold admission and never hold a cache mutex across SQL or preparation. One owned cold-preparation mutex covers snapshot capture, blocking index construction and publication; cancellation cannot release it while the blocking builder still runs. A queued cancelled request leaves no pending-key entry. Publication is generation-monotonic: an old builder cannot replace newer cached data. Missing resources and preparation errors are not retained.

Limits:

- At most eight hierarchy entries and **16 MiB** of conservative retained allocation charges, including keys/indexes.
- At most 100,000 concepts, 1,000,000 stored relationships and **32 MiB** of projected code/key bytes admitted to preparation. Bounded aggregate queries check the same snapshot before fetching rows.
- Plans exceeding input or retained-memory limits get a generation-scoped SQL-admission marker. Requests execute the existing fresh atomic SQL; no subset of the hierarchy is published. Oversized external code/canonical probes bypass preparation without truncating or accepting them.

The marker avoids rebuilding a known-oversized plan on every request. Cache pressure clears entries; existing immutable readers can finish independently. These are retention/preparation limits, not hard RSS, SQL-time or HTTP-queue budgets. Temporary SQL rows/build buffers and older in-flight snapshots need additional memory. Existing ValueSet and ConceptMap retention budgets remain 64 MiB and 16 MiB respectively.

## Correctness

All **38 Rust tests**, including six opt-in database scenarios, pass with repository migrations against the isolated PostgreSQL database. Workspace Clippy with warnings denied, formatting and all twelve benchmark-harness tests pass. All seven local semantic preflights pass on the final release. These are regression checks, not complete FHIR conformance.

Six new public-library tests cover outcomes, stored spelling/provider keys, invalid projections, repeated/self edges, direction precedence, immutable ownership and count limits. Two new database scenarios exercise concurrent cold readers, warm reads during paused cold admission, cancellation of a queued request, entry/byte budgets, eviction, projection-only commits and rollback, missing-code invalidation, version insertion/default selection, schema guard, TRUNCATE and deletion. A 10,001-concept folded projection checks all four outcomes and database Unicode normalization; a 100,001-concept projection verifies correct SQL admission fallback, including post-marker writes and oversized missing probes. The previous concurrent writer/readers test still requires every atomic code/closure generation to produce the correct positive result.

## Measurement protocol

Both term-squid variants use explicit **32-connection pools**. The baseline is the preserved default-32 atomic-SQL binary, not an old five-connection result. SQLx's return-to-pool health/recovery exchange is unchanged. Builds and intrusive diagnostics finish before load. Fresh processes run sequentially with a 5-second discarded warmup and 30-second windows at 1/10/50 VU; a separate 50-VU repeat reverses server order. Every response must pass the unchanged strict four-outcome TS-SS01 assertion, with 25% of each outcome and changing probes across all 10,000 child codes.

The local reference is FHIRsmith `25a41f87711507789b361480ee4efae42df504d4`, version 0.18.2 / txVersion 1.10.1, Node 24.16.0, default reference caching. Only the documented core-package pin and loopback binding change its bootstrap; query/worker code is unchanged. Both deployments load synthetic 1.0.0 and R4 core 4.0.1, with the documented legacy v2 import normalization. The known reference reverse-translation semantic mismatch remains visible and unscored.

Host: Apple M3 Pro, 12 cores, 36 GiB, macOS 26.7.1; PostgreSQL 17.10 in the shared 2-vCPU / 4,095,365,120-byte Docker VM. Client: Bun 1.4.4 and k6 2.3.0. This is a non-dedicated local deployment comparison, not a language-only or clinical performance claim. CPU is server-process time (100% = one core); RSS is the maximum one-second sample and excludes PostgreSQL/VM/client costs.

## Original workload results

| VU | Atomic SQL RPS | Prepared RPS | Reference RPS | Prepared / SQL | Prepared / reference |
|---|---:|---:|---:|---:|---:|
| 1 | 1,528.8 | 1,581.2 | 4,074.4 | 1.034x | 0.388x |
| 10 | 4,740.2 | 5,796.1 | 8,892.1 | 1.223x | 0.652x |
| 50 | 5,498.2 | 6,313.0 | 9,384.0 | 1.148x | 0.673x |

The primary gain is 3.4–22.3% over SQL, not a reference win. At 1 VU, prepared p95/p99 are slightly worse despite higher throughput. The reverse-order 50-VU repeat records SQL **6,029.2**, prepared **6,260.3**, reference **10,095.9** RPS: only **3.8%** better than SQL, and **0.620x** the reference. No best-peak substitution or aggregate product score is used.

All primary and repeat latency points remain visible:

| Group | Server | VU | p50 / p95 / p99 ms |
|---|---|---:|---|
| main | baseline | 1 | 0.544 / 0.759 / 1.280 |
| main | baseline | 10 | 1.835 / 3.273 / 4.997 |
| main | baseline | 50 | 8.378 / 13.587 / 19.692 |
| main | candidate | 1 | 0.519 / 0.775 / 1.303 |
| main | candidate | 10 | 1.494 / 2.584 / 4.285 |
| main | candidate | 50 | 7.039 / 12.377 / 19.226 |
| main | reference | 1 | 0.171 / 0.260 / 0.821 |
| main | reference | 10 | 0.890 / 1.968 / 3.775 |
| main | reference | 50 | 4.779 / 8.970 / 13.577 |
| repeat | baseline | 50 | 7.374 / 12.826 / 19.767 |
| repeat | candidate | 50 | 7.297 / 11.756 / 19.375 |
| repeat | reference | 50 | 4.571 / 7.932 / 11.589 |

| Group | VU | SQL CPU % / RSS MiB | Prepared CPU % / RSS MiB | Reference CPU % / RSS MiB |
|---|---:|---|---|---|
| main | 1 | 19.9 / 11.1 | 21.2 / 14.1 | 54.0 / 411.0 |
| main | 10 | 91.8 / 14.0 | 100.4 / 14.3 | 101.7 / 338.3 |
| main | 50 | 106.6 / 23.1 | 110.9 / 23.5 | 101.2 / 342.4 |
| repeat | 50 | 109.9 / 23.4 | 113.7 / 25.2 | 103.7 / 415.2 |

## Large folded workload

A separate fixture copies the original 10,001-concept hierarchy, changes only its canonical/id/name and sets `caseSensitive: false`. Its canonical is `http://example.org/term-squid-benchmark/hierarchy-folded`, version 1.0.0. The same 25% four-outcome sampler lowercases both query codes, so stored `C00007` is probed as `c00007`. Outcome checks are unchanged. This extra fixture is imported through the CLI and loaded from the identical JSON by the reference folder provider **after** the original matrix finishes. It is not silently included in the original dataset fingerprint.

The prepared server passes all four probes. The pinned reference returns `not-subsumed` for both `root -> c00007` and the reverse, instead of the required strict directions; only equivalence and unrelated siblings pass. Its failed preflight and complete responses are retained. **No reference throughput is scored for this workload**, and its speed cannot establish a reference win. The previous SQL baseline is measured separately with the same probes and capacity.

A separate fresh-reference diagnostic confirms that original-case `root -> C00007` and its reverse produce the correct strict directions, while the lowercase equivalents return `not-subsumed`. This local discrepancy is not worked around by changing expected outcomes or scoring the wrong answers.

| Server | VU | RPS | p50 / p95 / p99 ms | CPU % | RSS MiB |
|---|---:|---:|---|---:|---:|
| baseline | 50 | 1,284.7 | 32.792 / 68.106 / 78.405 | 19.2 | 22.9 |
| candidate | 50 | 7,289.5 | 6.399 / 9.323 / 13.780 | 130.9 | 25.1 |

This one folded pair improves throughput **5.674x** and reduces p99 from **78.405 to 13.780 ms** relative to our SQL baseline. It is not a reference speedup or a confidence interval. The original and folded results are different workloads and are not pooled. Higher Rust-process CPU accompanies more completed work; PostgreSQL CPU is outside the process observer, so no total CPU-cost reduction is inferred.

Across **14 scored measurements**, all **2,363,366 response checks** pass with zero HTTP errors and zero failed checks. The reference folded preflight fails separately and has no load result. Warmups and preflights are excluded from this passing-check count.

## Reproduction and provenance

Apply all repository migrations to the isolated database (`just bench-migrate`), build a separate release candidate and preserve the baseline binary. Start each with `DATABASE_MAX_CONNECTIONS=32`, `SERVER_HOST=127.0.0.1`, port 18081 and request logging disabled. Use the pinned reference/content setup from the earlier reports. Never apply load to public terminology servers.

For the original matrix, with a fresh process for each server:

```sh
python3 benchmarks/observe.py --pid SERVER_PID --output .benchmarks/UNIQUE-process.json -- \
  bun benchmarks/bench.ts run --tests TS-SS01 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import)'
```

Use `--name fhirsmith --url http://127.0.0.1:17003/tx/r4 --server-revision 25a41f87711507789b361480ee4efae42df504d4` for the reference. Repeat at `--vus 50` in reverse server order. A separate all-scenario preflight passes seven term-squid scenarios; the reference keeps its known reverse-translation failure.

To recreate the folded fixture, take `resources()[0]` from `benchmarks/local/cases.js`, set `id` to `benchmark-hierarchy-folded`, `url` to the folded canonical, `name` to `TermSquidBenchmarkFoldedHierarchy`, and `caseSensitive` to false; keep all concepts, parent properties and version unchanged. Save the JSON under `.benchmarks/`, import it with CLI `create-code-system`, and copy that exact file to the reference's folder source. Restart both servers. Adapt only TS-SS01 request URLs: replace the system canonical and lowercase `codeA`/`codeB`; reuse its original probes, sampler and response checker. Run `local/load.js` against that adapter with `TEST_ID=TS-SS-FOLDED`, `BASE_URL`, `VUS`, and `RESULT_FILE`, using one discarded 5-second/1-VU warmup and a 30-second/50-VU window. Require all four semantic probes first; do not measure a failing server. The ignored driver/manifests record the concrete adapter and preflight source hashes.

Identifiers:

- Baseline binary SHA-256: `05b0157e24736067c9f0fd4784669449d0786fbbd15825142e1ba29815fd4498`.
- Prepared binary SHA-256: `67913ae612e58e3addd804688d6457718b1a5def5c619c6d347f18d3b6e2023e`.
- Implementation source archive before report edits: `4807065a8851759ee68e04ee5ad1f94ee7aaa90b56343b836b2b90421a8aef0d`.
- Original synthetic SHA-256: `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`.
- Unchanged original harness SHA-256: `732026ed5adb0e59bf9fa50c46568c348ea6d262bff84dd866a34510aaba8399`.
- Additional folded fixture SHA-256 (pretty JSON plus final LF): `bddb882a51243619048630da3095432f767d54d9db3da2bf25f7b485096c1393`.

All raw settings, source snapshots, process samples, logs, reference failures and derived summaries remain ignored under `.benchmarks/profiles/hierarchy/` and `.benchmarks/results/hierarchy-*`. The failed reference folded run is preserved; a separate continuation finishes the SQL baseline without overwriting it. These artifacts do not travel with Git; regenerate the content/drivers on a clean machine. No commit, push or deployment was performed.

## Remaining work

Generation/protocol overhead still limits the original warm workload; lookup and translation also need their own measured work. Any shared/batched observation must start its SQL snapshot after every request admitted to that batch—joining a pre-existing observation or adding a TTL can violate post-commit freshness. Admission must stay bounded and cancellation-safe.

Dependency-specific generations, shared CodeSystem snapshots, cold preparation/update/import costs, clinical-sized closures, mixed workloads, arrival-rate/overload behavior and stable-machine repetitions remain unverified. The new projection triggers add write work and globally invalidate all plan families. Do not infer full conformance, operational memory bounds or a universal default-32 speed guarantee from these synthetic warm reads.
