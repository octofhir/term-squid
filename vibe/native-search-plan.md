# Native SNOMED synonym expansion search

## Task

Extend native `$expand` text filtering to active RF2 synonyms accepted in the selected release language and dialect. Keep code/display matching, projected display, composition, versions, totals and deterministic pages unchanged. No clinical corpus, comparison-identity policy, projection rebuild or performance claim is included.

## Plan structure

Implement the SQL predicate, test provider semantics, publish the actual search policy, then verify the workspace and update the handoff. This is standalone Git: do not rebase, commit, push or deploy.

## Execution plan

### Stage 1: Extend the native text predicate

**What to add/implement:**

- In `query_options`, match active synonym descriptions in `snomed_releases.language` with active preferred/acceptable members in `snomed_releases.dialect`.
- Use correlated `EXISTS` so matching descriptions do not multiply concepts. Keep the original selected display and literal substring policy. Apply PostgreSQL `lower` to both operands of native text comparisons; leave code identity unchanged.
- Use the existing repeatable-read transaction, statement/native deadlines and page budget. Do not load descriptions into a release-sized Rust plan.

**Files to edit/create:**

- `crates/backend/src/store/native_valueset.rs` — extend the expansion-only predicate.

**Framework/Library documentation:**

- [FHIR R4 expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html) — server-defined `filter` policy and capability documentation.
- [SNOMED CT in FHIR R4](https://hl7.org/fhir/R4/snomedct.html) — editions, language reference sets and display terms.

**Examples in existing code:**

- `crates/cli/src/snomed.rs` — selected-dialect projection.
- `crates/backend/src/store/designations.rs` — RF2 description access.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
```

### Stage 2: Add native search regression tests

**What to add/implement:**

- Test preferred/acceptable synonyms, excluded description/member/type/language/dialect states, duplicate matches and literal punctuation.
- Test imported intersections, exclusions, versions, totals, stable pages, zero count, updates and rollback. Assert projected display and non-search membership remain unchanged.

**Files to edit/create:**

- `crates/backend/src/native_search_tests.rs` — generated isolated PostgreSQL/HTTP fixtures.
- `crates/backend/src/tests.rs` — register the test module.

**Examples in existing code:**

- `crates/backend/src/native_valueset_tests.rs` — native fixture and operation checks.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline -p term-squid-backend native_search_tests -- --include-ignored
```

### Stage 3: Describe the supported search policy

**What to add/implement:**

- Describe selected-release synonym scope and literal matching in `TerminologyCapabilities.expansion.textFilter` and the API requirements.
- Add a metadata regression assertion without weakening existing assertions.

**Files to edit/create:**

- `crates/backend/src/api/resources/capability.rs` — search policy string.
- `docs/API_REQUIREMENTS.md` — provider-specific search behavior.
- `crates/backend/src/native_search_tests.rs` — metadata check.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline -p term-squid-backend native_search_tests -- --include-ignored
```

### Stage 4: Verify and record the increment

**What to add/implement:**

- Run workspace, independent core and unchanged harness checks. Record exact results and remaining native/clinical/Unicode limits.
- Keep raw output under `.benchmarks/native-search-20261106/`. Update continuation documents; do not replace historical measurements or imply clinical composite progress from generated fixtures.

**Files to edit/create:**

- `docs/NATIVE_SEARCH_REPORT.md` — implementation, validation and limits.
- `docs/NEXT_SESSION.md`, `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md` — current increment and next steps.
- `vibe/native-search-plan-track.md` — verified stage status.

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
