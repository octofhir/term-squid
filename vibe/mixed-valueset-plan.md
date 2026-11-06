# Bounded mixed-provider ValueSets

## Task

Allow already supported native SNOMED/LOINC selections to compose with complete finite CodeSystems, request-local finite sources and explicit bundled UCUM lists. Keep the reusable finite engine responsible for finite semantics and the native SQL compiler responsible for set composition and final bounded pages. No source publication, release-sized native Rust plan, new clinical corpus or performance claim is included.

## Plan structure

Implement the transaction-local finite relation bridge and rich adapters, add independent PostgreSQL/HTTP regression cases, then run all existing checks and update the continuation documents. Preserve the preceding uncommitted native-search increment. This is standalone Git; do not rebase, commit, push or deploy.

## Execution plan

### Stage 1: Integrate bounded finite leaves with native composition

**What to add/implement:**

- Add `compose_loader::load_selection` to load a single system selection without imports through the existing finite loader. Keep imported sets in the original native graph and never expose the internal synthetic root as provenance.
- Add operation-local `finite_bridge::Bridge`: canonical/actual-business-version identities, finite rows, source knowledge and page-local metadata. Resolve stored, temporary and explicit UCUM sources with existing date/version/supplement rules.
- Enforce aggregate finite source bytes, concept count, row count and conservative bridge-memory admission. Reject the whole operation when a limit or source rule fails.
- Move the existing owned native work permit into finite `spawn_blocking` preparation and return it afterwards. Cancellation must not release CPU admission before detached finite work actually stops.
- Add SQL relations for finite rows and selected source metadata. Preserve canonical-code equality in intersections/exclusions, last-include display precedence and whole-system/version scope. Apply Rust case-policy membership and finite text matching only after canonical set operations.
- Carry abstract/inactive/post-coordination flags and apply all result filters before total/page selection. Keep pure finite dispatch and native projection guards.
- Extend `ValueSetPage` with finite page metadata/knowledge. Use it in rich expansion, nesting, validation labels/diagnostics and translation scope membership without re-resolving temporary or UCUM sources through persisted metadata.

**Files to edit/create:**

- `crates/backend/src/store/finite_bridge.rs` — operation-local loader, preparation, limits, identity and metadata.
- `crates/backend/src/store/compose_loader.rs` — single-selection loader and bounded source accounting.
- `crates/backend/src/store/native_valueset.rs` — mixed SQL relations, selected-source identity and final filters.
- `crates/backend/src/store/value_set.rs`, `mod.rs`, `implicit_membership.rs` — finite page metadata and module registration.
- `crates/backend/src/store/postgres.rs` — owned native work permit.
- `crates/backend/src/store/expansion.rs`, `validation.rs`, `translation.rs` — rich mixed results and permit lifecycle.

**Framework/Library documentation:**

- [FHIR R4 ValueSet composition](https://hl7.org/fhir/R4/valueset.html#compositions) — union, intersections, exclusions and system/version/code uniqueness.
- [FHIR R4 expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html) — filtering, lifecycle, designations and pagination.

**Examples in existing code:**

- `crates/backend/src/store/compose_loader.rs::prepare_rich` — finite preparation and source knowledge.
- `crates/backend/src/store/validation.rs`, `translation.rs` — take/return ownership around bounded CPU work.
- `crates/backend/src/store/native_valueset.rs::Compiler` — native set operations and counted page.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add independent mixed-provider regression tests

**What to add/implement:**

- Add positive/negative generated fixtures for native/finite/UCUM unions, imported intersections, versions, display precedence, all exclusion scopes and exact canonical-code identity across case policies.
- Verify temporary overrides, contained/imported scope, request release, finite hierarchy/property/regex semantics, supplements, designations, abstract/inactive/UI/post-coordination options, nested output and object validation.
- Verify deterministic filtered totals/pages, zero count, freshness/rollback, cache parity, explicit incomplete/unknown/unsupported errors and aggregate work admission without partial success.
- Preserve every earlier assertion and benchmark pool/weight.

**Files to edit/create:**

- `crates/backend/src/mixed_valueset_tests.rs` — generated isolated PostgreSQL/HTTP scenarios.
- `crates/backend/src/tests.rs` — module registration.

**Examples in existing code:**

- `crates/backend/src/native_valueset_tests.rs`, `transient_tests.rs`, `ucum_finite_tests.rs`, `api_completion_tests.rs`.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline -p term-squid-backend mixed_valueset_tests -- --include-ignored
```

### Stage 3: Verify delivery and update continuation

**What to add/implement:**

- Run all workspace/database, independent-core and unchanged harness checks. Keep raw evidence under `.benchmarks/mixed-valueset-20261106/`.
- Record supported provider combinations, exact aggregate limits, cancellation ownership, Unicode policy boundaries and remaining clinical/provider gaps.
- Update the tracker and handoff without replacing historical ecosystem/performance evidence or claiming a clinical composite.

**Files to edit/create:**

- `docs/MIXED_VALUESET_REPORT.md` — implementation, reproduction, actual validation and limits.
- `docs/API_REQUIREMENTS.md`, `NEXT_SESSION.md`, `DEVELOPMENT_HANDOFF.md`, `PRODUCT_PLAN.md` — current support and next work.
- `vibe/mixed-valueset-plan-track.md` — verified stage status.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
git diff --check
```
