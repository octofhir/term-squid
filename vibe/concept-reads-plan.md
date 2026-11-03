# Prepared CodeSystem reads

## Task

Use immutable indexes over stored concept projections for lookup and CodeSystem validation, sharing fresh generation observations. Preserve existing version/instance/case/content/error behavior and atomic update visibility. Keep bounded admission and the original SQL as an exact fallback.

## Execution plan

### Stage 1: Add the storage-independent projection index

**What to add/implement:**

- Add `ProjectedConcept`, `ProjectedConcept::memory_charge`, `PreparedConcepts::from_rows`, `resolve` and `memory_charge`.
- Accept provider-defined comparison keys, reject duplicate/ambiguous input and bound concepts at 100,000. Preserve display, definition and JSON property values without interpreting or changing them.

**Files to edit/create:**

- `crates/core/src/concepts.rs` — immutable index and conservative iterative JSON charges.
- `crates/core/src/lib.rs` — public exports.

**Examples in existing code:**

- `crates/core/src/hierarchy.rs` — provider keys and immutable resolution.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline -p term-squid-core`

### Stage 2: Test the public library contract

**What to add/implement:**

- Test exact/folded/missing probes, stored spelling and payload preservation, duplicate/bad keys, count limits, memory charges and retained ownership.

**Files to edit/create:**

- `crates/core/src/concepts.rs` — public-API tests.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline -p term-squid-core`

### Stage 3: Add bounded PostgreSQL snapshots

**What to add/implement:**

- Add the Concept family to `GenerationReads`, using PostgreSQL query folding and the existing concept-projection tracking guard.
- Resolve metadata and bounded concept rows in one read-only repeatable-read transaction, then decode/build in blocking work while retaining owned cold admission.
- Retain at most eight entries / 32 MiB. Bound raw input at 32 MiB, a single property JSON at 64 KiB and metadata at 64 KiB; check decoded payload charges incrementally before accumulating the complete plan.
- Retain generation-scoped SQL markers for over-limit systems, not partial indexes. Missing systems and preparation errors stay uncached. Keep monotonic publication, oversized-probe SQL bypass and no stale SQL-error fallback.
- Route existing public `resolve_concept`/`resolve_concept_by_id` through the cache without changing their signatures or HTTP result handling. Keep the original SQL unchanged: current forced-generic diagnostics already use both code indexes, not a full-system scan.

**Files to edit/create:**

- `crates/backend/src/store/concepts.rs` — snapshots, limits and cache.
- `crates/backend/src/store/generation.rs` — Concept family/probe.
- `crates/backend/src/store/postgres.rs` — routing and test-only synchronization hooks.
- `crates/backend/src/store/mod.rs` — module registration.

**Framework/library documentation:**

- Cached SQLx 0.8.6 transaction/query implementation and Tokio 1.47.1 owned mutex/spawn_blocking documentation.
- `docs/HIERARCHY_REPORT.md`, `docs/GENERATION_BATCHING_REPORT.md` — freshness, immutable snapshots and cancellation.

**Examples in existing code:**

- `crates/backend/src/store/hierarchy.rs` — generation checks, bounded projections and SQL admission markers.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace`

### Stage 4: Verify HTTP/store freshness and limits

**What to add/implement:**

- Cover business versions, default/instance selection, complete/fragment errors, case policy and unchanged display/definition/properties behavior through public APIs.
- Exercise projection-only commits, rollback, atomic paired metadata/concept updates, deletion/TRUNCATE and schema errors after warmup.
- Verify warm reads during paused cold admission, queued cancellation, retained results after eviction, large folded data, count/property/decoded-memory SQL fallback and oversized probes.
- Keep all existing batching, overload, recovery and operation assertions enabled.

**Files to edit/create:**

- `crates/backend/src/tests.rs` — integration scenarios.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `bun test benchmarks`

### Stage 5: Matched measurements and handoff

**What to add/implement:**

- Preserve the current generation-batching binary as baseline and build a separate release candidate. Finish builds/diagnostics before timing.
- Require all seven local semantic gates, then compare unchanged TS-CS01/TS-LK01 on fresh baseline/candidate/pinned-reference processes, explicit pool 32, 1/10/50 VU, 5-second warmup and 30-second windows. Repeat loaded lookup in reverse server order and preserve losses.
- Record CPU/RSS, errors, fixture/harness/source hashes and unchanged migration requirements; update the handoff without claiming overall 3x or clinical conformance.

**Files to edit/create:**

- `docs/CONCEPT_READS_REPORT.md` — implementation, evidence and reproduction.
- `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`, `benchmarks/README.md` — current state and next work.

**Verification commands:**

- `cargo fmt --all -- --check`
- `git diff --check`
- `bun test benchmarks`
- Run the matched local protocol and audit all response checks.
