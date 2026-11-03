# Fresh generation batching

## Task

Batch overlapping warm generation checks without reusing an observation whose SQL snapshot predates a request. Preserve transactional cold snapshots, PostgreSQL hierarchy keys, schema guards and SQLx pool recovery. Bound queued work and keep the previous hierarchy binary as the explicit-pool-32 baseline.

## Plan structure

Implement the shared observer, verify request boundaries/cancellation/failures, then run matched measurements and update the handoff.

## Execution plan

### Stage 1: Add bounded shared observation

**What to add/implement:**

- Add `GenerationReads` with one cancellation-independent worker and a maximum of 256 pending requests plus one bounded active batch.
- Acquire a pool connection, detach all current requests, then issue one autocommit generation/guard/key-folding statement. Requests arriving after detachment wait for a later statement; retain no completed observation.
- Share database errors without stale fallback. Reject a full live queue with HTTP 503 OperationOutcome. Cancelled queued recipients free admission; worker shutdown releases waiters.
- Route warm ValueSet, ConceptMap and hierarchy checks through the shared observer; retain direct generation reads inside cold repeatable-read transactions.

**Files to edit/create:**

- `crates/backend/src/store/generation.rs` — observer, bounded queue and worker lifecycle.
- `crates/backend/src/store/mod.rs` — module registration.
- `crates/backend/src/store/postgres.rs` — shared observer and warm call sites.
- `crates/backend/src/store/hierarchy.rs` — shared warm observation with unchanged cold normalization.
- `crates/backend/src/error.rs` — shared database failure and overload response.

**Framework/library documentation:**

- Cached Tokio 1.47.1 `src/task/spawn.rs`, `src/sync/oneshot.rs` — detached tasks and receiver cancellation.
- Cached SQLx 0.8.6 pool/connection implementation — normal checkout, return health/recovery and cancellation.
- `docs/HIERARCHY_REPORT.md` — snapshot boundary and retained-plan contracts.

**Examples in existing code:**

- `crates/backend/src/store/hierarchy.rs` — fresh observation and cold admission.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace`

### Stage 2: Verify public behavior and admission

**What to add/implement:**

- Add public store/HTTP regression scenarios with test-only dispatch/delivery gates and counters. Verify batching, family guards, ordered PostgreSQL keys, full-queue rejection, cancellation and recovery.
- Hold delivery of a completed old observation, commit an independent update, then enqueue a new request: only the old operation may see the old state.
- Keep all previous version, projection, rollback, atomic update and cache-limit checks enabled.

**Files to edit/create:**

- `crates/backend/src/tests.rs` — integration scenarios and existing public-API assertions.
- `crates/backend/src/store/postgres.rs` — test-only synchronization accessors.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `bun test benchmarks`

### Stage 3: Measure and hand off

**What to add/implement:**

- Preserve baseline/candidate binaries and source fingerprints. Finish builds before timing; require all seven local semantic preflights.
- Compare the four prepared workloads TS-VC01/TS-EX01/TS-SS01/TS-CM01 on fresh baseline/candidate/pinned-reference processes at explicit pool 32, 50 VU, 5-second warmup and 30-second windows. Add 1/10 VU for TS-SS01. Keep reverse reference translation and folded subsumption failures unscored.
- Preserve all points and repeat the loaded TS-SS01 point in reverse server order. Record CPU/RSS, errors, queue limits, unchanged migrations and remaining gaps.

**Files to edit/create:**

- `docs/GENERATION_BATCHING_REPORT.md` — evidence and reproduction.
- `docs/DEVELOPMENT_HANDOFF.md` — current state and next work.
- `docs/PRODUCT_PLAN.md` — increment progress.
- `benchmarks/README.md` — observer/admission protocol.

**Verification commands:**

- `cargo fmt --all -- --check`
- `git diff --check`
- `bun test benchmarks`
- Run the matched local protocol and audit every selected response check.
