# Built-in UCUM provider

## Task

Reuse the published `octofhir-ucum` library already selected by the local FHIRPath project. Add grammar-backed canonical lookup/CodeSystem validation and a reusable library comparability API, not a finite unit whitelist. This increment does not implement infinite ValueSet enumeration or new nonstandard HTTP operations.

## Execution plan

### Stage 1: Provider and canonical routing

**What to add/implement:**

- Pin `octofhir-ucum` to 0.9.1 with default features disabled; use its bundled UCUM 2.2 registry, parser, evaluator and display/comparability functions.
- Add `UcumProvider::lookup` and `UcumProvider::comparable` in the core. Bound expression size, preserve case sensitivity, reject unsupported versions and distinguish library limits from invalid units.
- Route canonical UCUM concept resolution through the immutable provider, independent of database content/generation. Preserve all other stored and instance behavior.
- Declare the built-in version in TerminologyCapabilities.

**Files to edit/create:**

- `crates/core/Cargo.toml`, `Cargo.lock`, `crates/core/src/ucum.rs`, `crates/core/src/lib.rs` — reusable provider.
- `crates/backend/src/store/postgres.rs`, `api/resources/capability.rs` — canonical adapter and capabilities.

**Framework/library documentation:**

- Published `octofhir-ucum@0.9.1` source: `src/lib.rs`, `src/parser.rs`, `src/evaluator.rs`, `src/error.rs`, bundled `ucum-essence.xml`.
- https://github.com/octofhir/ucum-rs

**Examples in existing code:**

- `crates/backend/src/store/postgres.rs::resolve_concept` — canonical context.
- `../fhirpath-rs/Cargo.toml` — existing selection only; do not add a sibling checkout dependency.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --workspace`

### Stage 2: Provider and HTTP regression tests

**What to add/implement:**

- Test atomic, prefixed, compound, annotated, special and invalid/case-sensitive expressions; version and resource limits; comparable/incomparable/invalid operands.
- Test canonical GET/POST lookup and validation, unknown codes and versions, metadata, and no database requirement. Keep stored/instance operations unchanged.

**Files to edit/create:**

- `crates/core/src/ucum.rs` — public provider tests.
- `crates/backend/src/tests.rs` — actual router tests.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace --offline -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --offline --all-targets -- -D warnings`

### Stage 3: Release smoke and handoff

**What to add/implement:**

- Build and smoke the release server locally: prefixed/compound lookup, positive/negative validation and unavailable-version errors. Run unchanged local/upstream preflights.
- Document dependency/model versions, supported API, input limits and remaining ValueSet/instance gaps. Keep original benchmark pools and clinical coverage honest.

**Files to edit/create:**

- `docs/UCUM_PROVIDER_REPORT.md`, `docs/PRODUCT_PLAN.md`, `docs/DEVELOPMENT_HANDOFF.md` — contract and verification.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo build --release --offline --bin backend`
- Local release HTTP smoke and unchanged preflight suites.
- `cargo fmt --all -- --check`
- `bun test benchmarks`
- `git diff --check`
