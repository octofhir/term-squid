# Development Handoff

Updated November 3, 2026. This file is intended for an agent starting from a clean checkout on another computer.

## Read first

For a new session on another device, follow [the resume checklist](NEXT_SESSION.md). It lists what is committed, what must be recreated and the next native clinical ValueSet work.

1. [Product plan](PRODUCT_PLAN.md): scope, milestones, performance objectives and the branded UI roadmap.
2. [Original performance report](PERFORMANCE_REPORT.md), [prepared data](PREPARED_DATA_REPORT.md), [warm reads](WARM_READ_REPORT.md), [pool/transport experiment](TRANSPORT_PERFORMANCE_REPORT.md), [prepared translation](TRANSLATION_PERFORMANCE_REPORT.md), [prepared hierarchies](HIERARCHY_REPORT.md), [fresh generation batching](GENERATION_BATCHING_REPORT.md) and [prepared CodeSystem reads](CONCEPT_READS_REPORT.md).
3. [Benchmark instructions](../benchmarks/README.md): isolated PostgreSQL, synthetic fixtures, semantic checks, profiling and load testing.
4. [HealthSamurai composite baseline](HEALTHSAMURAI_BASELINE.md) and [HTS feature audit](HTS_FEATURE_GAPS.md): current primary KPI, twenty-test coverage and prioritized gaps.

The server has its own database and a reusable `term-squid-core` engine. Local code systems, HL7, LOINC and UCUM are the product priorities. Embedding the engine/database in OctoFHIR is outside the current scope. The primary objective is original HealthSamurai composite leadership with preserved semantics, including direct local Termbox/FHIRsmith comparisons. The overall 3x reference goal remains unmet. Reports now demonstrate >12,000 RPS on five synthetic warm operations at 50 VU across the batching and CodeSystem-read increments. The latest lookup/CodeSystem-validation comparison is 16,949 / 18,408 RPS versus 8,325 / 5,105 reference. Single-client, clinical/mixed-load and overall-suite targets remain open.

## Included implementation

- Immutable prepared ValueSet plans, indexed membership, bounded retention and concurrent warm generation checks.
- Immutable forward/reverse ConceptMap indexes in the library, shared string buffers and bounded map-plan retention.
- Transactional generation tracking for CodeSystem, ValueSet, ConceptMap, concept and closure writes, including changes made by separate CLI processes. No generation TTL or stale-answer fallback.
- Migration `20261102000003` adds ConceptMap tracking. New migration **`20261103000000`** adds concept/closure tracking and the `hierarchies_tracked` guard. Apply all migrations before starting the new server; startup does not run migrations automatically.
- The backend pool skips redundant hot-checkout pings while checking idle sockets. SQLx's return-to-pool health check remains. TCP_NODELAY variants were experiments and are not selected in production source.
- The backend now defaults to **32 connections per process**, chosen against 5 and 16 on all six shared warm-read scenarios. Explicit values 1–64, including five, remain supported. A rebuilt binary confirms the fallback without an environment override, and passes longer matching reference runs; see [default selection](POOL_DEFAULT_REPORT.md). Budget database sessions across replicas, CLI imports and administration. Historical five-connection results require an explicit override.
- Bounded immutable hierarchy plans now resolve subsumption through `term-squid-core`, preserving PostgreSQL case normalization, stored projections, version/instance selection and fresh per-operation generation checks. The cache has eight entries / 16 MiB; over-limit systems retain an admission marker and use the atomic SQL path. Its pre-batching 50-VU comparison reaches 6,313 RPS versus 5,498 SQL and 9,384 reference; the repeat narrows the SQL gain to 3.8%. A separate folded hierarchy improves 1,285 → 7,289 RPS, but the reference fails its semantic gate and is unscored. See [implementation and measurements](HIERARCHY_REPORT.md). No TTL, SQLx health-check removal or pool-size change is used for this increment.
- A bounded shared generation observer now batches overlapping ValueSet, ConceptMap and hierarchy checks without TTL or joining a statement dispatched before the request arrived. There are at most 256 pending observations plus one active batch; overload returns HTTP 503, cancellation frees queued capacity, and database failures never return stale plans. Cold repeatable-read snapshots, schema guards and SQLx health/recovery remain. At 50 VU it reaches 19,648 membership / 8,739 expansion / 18,157 subsumption / 18,228 forward-translation RPS; all four beat the pinned reference at that concurrency. Subsumption still loses at 1/10 VU, and the 1-VU candidate is 2.7% below its baseline. A reverse-order 50-VU repeat confirms 17,487 versus 9,964 reference RPS. See [the batching report](GENERATION_BATCHING_REPORT.md). No new migration or dependency is required.
- Immutable `PreparedConcepts` now serves stored lookup/CodeSystem-validation payloads through the shared fresh observer. The new cache has eight entries / 32 MiB (128 MiB total across all four cache budgets), bounded raw/decoded admission and original-SQL markers for oversized systems. Metadata, case policy and payloads use one cold snapshot; complete/fragment errors and version/instance selection are unchanged. At 50 VU lookup improves 5,819 → 16,949 RPS (reference 8,325), validation 5,631 → 18,408 (reference 5,105). The lookup repeat reaches 21,265 versus 8,842 reference; retain its 16,949–21,265 range. Single-client reference losses and the 10-VU validation tail regression remain. See [implementation and measurements](CONCEPT_READS_REPORT.md). No new migration or dependency is required.
- The CLI narrowly normalizes six malformed v2 CodeSystems and their ValueSet references only in `hl7.fhir.r4.core#4.0.1`. The unchanged official archive now imports all 2,458 terminology resources; strict engine version-conflict checks remain. See [R4 core compatibility](R4_CORE_COMPATIBILITY.md).
- Its benchmark follow-up checks all four subsumption outcomes with a stable 25% mix, records local harness source fingerprints and adds a separate Bun harness-test CI job. All twelve harness tests pass locally. Fixture content is unchanged, but historical positive-only TS-SS01 results are not a matched baseline for this new request mix.
- The previous accumulated increment was pushed as `839e14b`; the prepared ConceptMap implementation and UI-plan additions are included in the snapshot containing this handoff.

The earlier CodeSystem-read increment passed **47 Rust tests including ten database scenarios**, workspace Clippy with warnings denied, formatting, twelve benchmark harness tests and all seven local semantic scenarios. Coverage includes atomic metadata/payload snapshots, version/instance/case/content behavior, bounded cold preparation and SQL fallbacks, projection updates/rollback/TRUNCATE, generation-batch boundaries, overload/cancellation and recovery. These are regression checks, not complete FHIR conformance. Historical performance records remain unchanged. The earlier hierarchy increment preserves fourteen scored measurements and its reference folded failure. The earlier batching comparison preserves 21 scored measurements / 5,284,983 checks. The current CodeSystem-read comparison adds 21 scored measurements / 4,399,050 passing checks; all losing points and memory increases remain in its report.

## Next development priorities

The latest [native RF2 increment](SNOMED_RF2_REPORT.md) adds checksummed streaming Snapshot import, native projection knowledge and bounded inferred subsumption. Migration **`20261103000001_snomed_rf2`** is required; apply it through `term-squid-cli migrate` before restarting the server. The workspace passes 64 Rust tests, including nineteen database scenarios. The next engine task is **native SNOMED ValueSet/implicit/filter evaluation**, not removing the finite-JSON rejection guard or treating retained roles as already implemented ECL. Clinical archives are still absent and no comparable clinical composite is claimed. See [NEXT_SESSION.md](NEXT_SESSION.md) for exact starting files and clean-machine commands.

The latest [UCUM increment](UCUM_PROVIDER_REPORT.md) pins `octofhir-ucum` 0.9.1 (model 2.2), exposing grammar-backed canonical lookup/CodeSystem validation without SQL and reusable Rust comparability. No custom parser or finite whitelist. Explicit stored versions and instances keep their own context. That increment passed 57 Rust tests, including fourteen database scenarios; real release smoke passes 8/8 UCUM and 6/6 inline checks. UCUM ValueSet integration and clinical composite coverage are still incomplete.

The subsequent [inline ValueSet increment](INLINE_VALUESET_REPORT.md) adds bounded request-local expansion/membership through fresh shared snapshots, content-fingerprinted cache keys and no persistence. All 51 Rust tests, including thirteen database scenarios, pass. Real HTTP smoke passes 6/6 and unchanged local gates 7/7; upstream remains 2/20 because clinical content/semantics are missing. The old five `valueSet` input rejections are fixed, not five new clinical passes.

**User-selected priority supersedes the older order below:** reach first place by the original HealthSamurai composite, directly compared with local Termbox and FHIRsmith. The benchmark integration now reproduces the five published scores using the original scorer, preserves canonical timing and offers a local graph site plus a complete-cohort scoring command. See the baseline report for commands and corpus/environment attestation limits. Current upstream preflight is 2/20 (FS01 and LK03); there is no comparable clinical score yet. The initial audit rejected five inline expansion inputs; the subsequent inline increment fixes that adapter gap, while their clinical checks still fail/skip; translation has an explicit R4 `target` compatibility discrepancy. Missing full clinical ingestion/SNOMED behavior is the next major workstream, not another synthetic pool tweak. HTS has broader features today; its audit is a scoped backlog, not a superiority claim.

The accumulated Rust/performance work was committed as **`f78fd24`**, without a push. That earlier benchmark/UI/audit-only increment changed no Rust implementation. Its 29 Bun tests, original-score reproduction, local full/strict/negative-only audits, generated-fixture scorer smoke, static site build and desktop/mobile browser checks pass. The browser renders twelve actual synthetic charts, with no JavaScript errors, external requests or horizontal page overflow. The local UI is started with `just bench-ui`, then opened at **http://127.0.0.1:4100/tx-benchmark/local/**. Generated site/dependencies/raw measurements do not travel with Git.

1. Connect selected R4 ecosystem cases to a reproducible resource-loading workflow and CI; maintain a parameter/semantic requirements matrix. Rich typed properties, Coding/CodeableConcept inputs, language/lifecycle/supplements and map version context remain incomplete. Synthetic gates do not establish release conformance.
2. Add clinical, cold/import/update, mixed and fixed-arrival-rate measurements. Reverse translation still needs its own matched term-squid timing; the reference mismatch remains unscored. Keep historical unstable repeats visible, explicit pool 32 and observer/response-memory limits; do not extrapolate warm synthetic peaks.
3. Extend shared snapshots and dependency-specific generations where profiles justify it. Prepared operations still depend on fresh database observations, so Milestone 2’s database-free API objective remains unmet. Preserve post-commit freshness, schema guards, bounded admission and SQLx recovery. Broader providers, including full UCUM and official LOINC content, remain separate work.
4. Follow Milestone 6 for the UI rewrite when that workstream is selected: match OctoFHIR branding and prefer a shared kit. The inspected `@octofhir/ui-kit` is private, source-exported and targets React 19; this frontend uses SolidJS. Decide framework and reproducible package delivery before changing screens. Do not introduce a hidden dependency on a sibling `server-rs` checkout or assume public npm publication.

For parallel work on another computer, use a separate branch and select a clear workstream to avoid duplicating concurrent engine/UI changes.

## Clean-machine setup and checks

Install the tools listed in the benchmark README, then use its setup workflow. For the isolated synthetic database:

```sh
just bench-setup

TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace -- --include-ignored

cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --all-targets -- -D warnings
bun test benchmarks
```

Use offline Cargo flags only after dependencies are cached. Start `just bench-server` separately and run `just bench-preflight --strict` before applying load. Keep builds and intrusive profiles separate from final timing runs; record RPS, p50/p95/p99, errors and available CPU/RSS. Never weaken semantic checks or load-test public terminology servers.

## What does not travel with Git

The original PostgreSQL volume, `.env`, generated fixtures, raw `.benchmarks/` profiles, measurement archives, instrumented binaries, local FHIRsmith checkout and unpublished OctoFHIR UI-kit source are not in this repository. Regenerate the synthetic dataset/database on the new machine. Historical reports and their summary tables are committed; local artifact paths and hashes are provenance, not files guaranteed to exist in a clean checkout.

Fresh reference comparisons require recreating the pinned local FHIRsmith setup. The reports identify its source revision and the bootstrap-only R4 core package pin used to bypass an unavailable package registry; query/worker code was unchanged. Do not silently replace this with a different reference/content version or mix measurements from different hardware into one speedup claim.
