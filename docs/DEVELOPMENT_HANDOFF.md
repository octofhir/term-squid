# Development Handoff

Updated November 3, 2026. This file is intended for an agent starting from a clean checkout on another computer.

## Read first

1. [Product plan](PRODUCT_PLAN.md): scope, milestones, performance objectives and the branded UI roadmap.
2. [Original performance report](PERFORMANCE_REPORT.md), [prepared data](PREPARED_DATA_REPORT.md), [warm reads](WARM_READ_REPORT.md), [pool/transport experiment](TRANSPORT_PERFORMANCE_REPORT.md) and [prepared translation](TRANSLATION_PERFORMANCE_REPORT.md).
3. [Benchmark instructions](../benchmarks/README.md): isolated PostgreSQL, synthetic fixtures, semantic checks, profiling and load testing.

The server has its own database and a reusable `term-squid-core` engine. Local code systems, HL7, LOINC and UCUM are the product priorities. Embedding the engine/database in OctoFHIR is outside the current scope. The objective is consistently outperforming FHIRsmith with preserved semantics; the overall 3x reference and >12,000 simple-read RPS goals remain unachieved.

## Included implementation

- Immutable prepared ValueSet plans, indexed membership, bounded retention and concurrent warm generation checks.
- Immutable forward/reverse ConceptMap indexes in the library, shared string buffers and bounded map-plan retention.
- Transactional generation tracking for CodeSystem, ValueSet and ConceptMap writes, including changes made by separate CLI processes. No generation TTL or stale-answer fallback.
- Migration `20261102000003` adds ConceptMap tracking and the `maps_tracked` guard. Apply all migrations before starting the new server; startup does not run migrations automatically.
- The backend pool skips redundant hot-checkout pings while checking idle sockets. SQLx's return-to-pool health check remains. TCP_NODELAY variants were experiments and are not selected in production source.
- The previous accumulated increment was pushed as `839e14b`; the prepared ConceptMap implementation and UI-plan additions are included in the snapshot containing this handoff.

The latest implementation passed 26 Rust tests including three database integration scenarios, workspace Clippy with warnings denied, formatting, nine benchmark harness tests and all seven local semantic preflights. These are regression checks, not complete FHIR conformance.

## Next development priorities

1. Address the largest remaining reference gap: subsumption currently resolves concepts and closure relationships through separate SQL reads. Investigate one atomic read or reusable prepared hierarchy snapshots, preserving version selection, case handling and update consistency.
2. Investigate generation-query/pool protocol costs and the unstable translation measurements. Prepared maps remove repeated full-resource decoding, but the primary reverse runs regressed while fresh same-protocol repeats improved. Preserve both results; do not select only the best peak. Any batching must preserve post-commit freshness and bounded admission.
3. Extend the R4 parameter/semantic contract and ecosystem coverage. Source/target map version context, richer properties, unsupported fallback modes and broader terminology providers remain incomplete. The reference reverse-translation mismatch remains unscored.
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
