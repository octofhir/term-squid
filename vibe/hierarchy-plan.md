# Prepared hierarchy increment

## Task

Resolve subsumption through a bounded immutable library index while preserving stored projection semantics, business-version/instance selection, PostgreSQL case folding and post-commit freshness. Retain the indexed atomic SQL path for plans that exceed admission limits. Keep pool capacity 32 on both sides of measurements.

## Plan structure

Library implementation and tests, transactional projection tracking and adapter, database regression coverage, then matched verification and evidence.

## Execution plan

### Stage 1: Add the immutable closure index

**What to add/implement:**

- Add `HierarchyConcept`, `PreparedHierarchy::from_closure`, borrowed code/relationship resolution and conservative `memory_charge` in the SQL-independent core.
- Bound concepts at 100,000 and materialized relationships at 1,000,000. Accept provider-generated comparison keys rather than applying a different Unicode case policy.

**Files to edit/create:**

- `crates/core/src/hierarchy.rs` — implement the index.
- `crates/core/src/lib.rs` — export its types.

**Examples in existing code:**

- `crates/core/src/translation.rs` — immutable indexes and shared string storage.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline -p term-squid-core`

### Stage 2: Test the library contract

**What to add/implement:**

- Test all outcomes, missing codes, provider comparison keys, invalid projections, limits and retained immutable ownership through public methods.

**Files to edit/create:**

- `crates/core/src/hierarchy.rs` — add public-API tests.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline -p term-squid-core`

### Stage 3: Add transactional projection tracking and cached reads

**What to add/implement:**

- Add concept/closure generation triggers and a `hierarchies_tracked` schema guard.
- Read metadata, bounded concepts and stored closure in one repeatable-read transaction. Prepare outside the transaction while retaining owned cold admission through blocking work.
- Check the generation on every warm operation. Never hold cache locks across SQL, cache missing resources, publish older generations, or use stale data after a query failure.
- Retain at most eight entries / 16 MiB of conservative charges. Bound projected input to 32 MiB; use a generation-scoped SQL-admission marker for oversized plans rather than an incomplete index.
- Keep existing store/HTTP interfaces and the atomic SQL path. Preserve error precedence and normalize queries with PostgreSQL.

**Files to edit/create:**

- `crates/backend/migrations/20261103000000_hierarchy_generation.up.sql` — add tracking.
- `crates/backend/migrations/20261103000000_hierarchy_generation.down.sql` — remove tracking transactionally.
- `crates/backend/src/store/hierarchy.rs` — implement snapshot preparation/cache.
- `crates/backend/src/store/mod.rs` — register the module.
- `crates/backend/src/store/postgres.rs` — route subsumption through it, retaining SQL admission fallback.

**Framework/library documentation:**

- `docs/PREPARED_DATA_REPORT.md` — transactional generation and immutable snapshots.
- `docs/WARM_READ_REPORT.md` — concurrent checks, monotonic publication and cancellation admission.

**Examples in existing code:**

- `crates/backend/src/store/postgres.rs` — prepared ConceptMap/ValueSet paths.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace`
- Apply all migrations through SQLx to the isolated benchmark database.

### Stage 4: Verify freshness, admission and bounds

**What to add/implement:**

- Extend public store/HTTP integration tests for projection-only commits, rollback, deletion, TRUNCATE, schema guard, version selection and concurrent updates.
- Exercise a large folded hierarchy, admission fallback, eviction, warm reads during paused cold admission and cancellation of a queued cold request.

**Files to edit/create:**

- `crates/backend/src/tests.rs` — add regression scenarios and minimal test hooks.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `bun test benchmarks`

### Stage 5: Measure and hand off

**What to add/implement:**

- Preserve the current default-32 binary as baseline. Build a separate release candidate; finish builds and intrusive profiles before timing.
- Require semantic gates, then compare the unchanged four-outcome workload on baseline, candidate and pinned local FHIRsmith with explicit pool 32, 5-second warmup and 30-second windows at 1/10/50 VU. Preserve repeats and losing points.
- Record CPU/RSS, errors, limits, migration requirements and remaining gaps; update handoff and product-plan progress without claiming full conformance or an unmeasured target.

**Files to edit/create:**

- `docs/HIERARCHY_REPORT.md` — implementation and evidence.
- `docs/DEVELOPMENT_HANDOFF.md` — current state and next priority.
- `docs/PRODUCT_PLAN.md` — increment link.
- `benchmarks/README.md` — migration and reproduction notes.

**Verification commands:**

- `cargo fmt --all -- --check`
- `git diff --check`
- `bun test benchmarks`
- Run the matched local benchmark protocol and audit every result's semantic checks.
