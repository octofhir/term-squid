# Inline ValueSet execution

## Task

Accept bounded request-local ValueSets for R4 `$expand` and `$validate-code`, using the existing compose engine and fresh dependency snapshots. Preserve stored operations and make temporary-resource cache identities depend on content, not just canonical URL.

## Execution plan

### Stage 1: Implement inline inputs and snapshots

**What to add/implement:**

- Add `Parameter.resource` and explicit inline selector/envelope validation; reject duplicate or conflicting root selectors and instance/inline mixtures.
- Add a bounded inline resource identity with SHA-256, business-version validation and no database writes.
- Reuse `compose_snapshot` for inline roots and stored dependencies. Add the inline fingerprint to the existing bounded plan-cache key; preserve fresh generation checks, repeatable-read, cold admission and all existing evaluator limits.
- Route expansion and membership POST inputs through `compose_inline`; preserve pagination/filter and system-version membership behavior.

**Files to edit/create:**

- `crates/backend/src/api/parameters.rs`, `api/operations/expand.rs`, `api/operations/validate.rs` — input normalization and routing.
- `crates/backend/src/store/inline.rs`, `store/mod.rs`, `store/postgres.rs`, `store/traits.rs` — inline identity and snapshot reuse.
- `crates/backend/Cargo.toml`, `Cargo.lock` — reuse the already locked SHA-256 dependency.

**Framework/library documentation:**

- https://hl7.org/fhir/R4/valueset-operation-expand.html
- https://hl7.org/fhir/R4/valueset-operation-validate-code.html
- https://hl7.org/fhir/R4/parameters.html

**Examples in existing code:**

- `crates/backend/src/store/postgres.rs::compose_snapshot` — bounded fresh plans.
- `crates/backend/src/api/operations/expand.rs::perform_expand` — deterministic pagination.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --workspace --offline`
- `cargo fmt --all -- --check`

### Stage 2: Regression tests

**What to add/implement:**

- Exercise public HTTP/store behavior: anonymous/named inline resources, include/exclude, imported intersections, typed filters, pagination and membership versions.
- Check cache isolation from stored roots and different inline payloads, bounded retention, fresh dependency commits/rollback, cycles, missing/incomplete content, malformed/duplicate/conflicting selectors, and input limits.
- Keep existing stored-operation gates unchanged.

**Files to edit/create:**

- `crates/backend/src/tests.rs` — isolated-schema HTTP and storage tests.

**Examples in existing code:**

- `crates/backend/src/tests.rs::hierarchy_test_database` — disposable PostgreSQL schema.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace --offline -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --offline --all-targets -- -D warnings`
- `bun test benchmarks`

### Stage 3: Local protocol recheck and handoff

**What to add/implement:**

- Build the release binary and run unchanged local semantic gates plus all original upstream preflight probes locally. Keep clinical missing-content failures visible; do not replace pools or claim full composite coverage.
- Add an actual local HTTP inline smoke with positive/negative membership and exclusion/pagination checks; retain raw results under `.benchmarks/`.
- Document the supported contract, limits and remaining clinical/filter/translation blockers; update the product plan/handoff and benchmark report.

**Files to edit/create:**

- `docs/INLINE_VALUESET_REPORT.md`, `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`, `docs/HEALTHSAMURAI_BASELINE.md`, `benchmarks/README.md` — implementation and evidence.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo build --release --offline --bin backend`
- `bun benchmarks/bench.ts run --preflight-only --strict`
- `bun benchmarks/bench.ts run --suite upstream --preflight-only`
- Local inline HTTP smoke.
- `cargo fmt --all -- --check`
- `git diff --check`
