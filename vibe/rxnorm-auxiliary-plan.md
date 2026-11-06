# RxNorm semantic types and normalized NDC

## Task

Extend the existing atomic monthly RxNorm import with optional complete `RXNSTY.RRF` and `RXNSAT.RRF` sources. Add real semantic-type lookup/filtering and explicitly scoped NLM-curated normalized NDC properties/filtering without inferring history, arbitrary attributes, mappings or concept equivalence.

## Execution plan

### Stage 1: Implement source cohorts, storage and provider semantics

**What to add/implement:**

- Add migration `20261106000001_rxnorm_auxiliary` with raw semantic-type/attribute tables, generation triggers and per-release loaded flags plus validated source descriptors. Existing releases stay auxiliary-unavailable without changing their fingerprint.
- Accept optional `semanticTypes`/`attributes` manifest descriptors containing path, SHA-256, exact row count, release version and full-monthly attestation. Load all declared files in the same transaction with existing streaming bounds.
- Validate exact six/thirteen-field formats, references and STY coverage; preserve raw fields/rows, including external/suppressed/unsupported attribute kinds. Project only supported current-active knowledge and guard aggregate per-concept JSON before construction.
- Preserve byte-identical CONSO-only fingerprint context. When optional sources are present, include their immutable hashes/count/version/context. Adding files to a published edition must conflict, never silently augment it.
- Add closed typed STY filter columns TUI/default, STN and STY; `=`/`in`, bounded lists and explicit malformed-input rejection. Add exact eleven-digit normalized `NDC` `=`/`in` without padding or source-code heuristics.
- Require the selected edition's verified source flag before any auxiliary query, even if another predicate would produce no rows. Explicit requested lookup properties require available source; primary/default/wildcard lookup returns available knowledge.
- Use release-scoped existential SQL predicates, preserving mixed set identity, totals/pages, current-active scope, snapshot consistency and existing work limits. Advertise capabilities only for available source files.
- Extend readiness with the new storage/triggers and source descriptors, without scanning clinical corpora. Safe downgrade locks publication targets and refuses to discard declared/raw auxiliary data.

**Files to edit/create:**

- `crates/backend/migrations/20261106000001_rxnorm_auxiliary.{up,down}.sql` — optional cohort storage and safe rollback.
- `crates/cli/src/rxnorm.rs`, `rxnorm/auxiliary.rs`, `rxnorm/reader.rs` — multi-source atomic import/projection.
- `crates/core/src/rxnorm.rs` — closed semantic-type/NDC filter grammar.
- `crates/backend/src/store/native_valueset.rs`, `lookup.rs`, `capabilities.rs` — provider predicates, availability and metadata.
- `crates/backend/src/api/operations/lookup.rs`, `transient_tests.rs` — requested-property context.
- `crates/backend/src/readiness.rs` — fresh source-registration checks.
- `benchmarks/rxnorm-auxiliary.template.json` — intentionally incomplete cohort template.

**Framework/Library documentation:**

- [NLM technical documentation](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html) — RXNSTY/RXNSAT columns, attachments, source keys, normalized NDC.
- [FHIR R4 RxNorm](https://hl7.org/fhir/R4/rxnorm.html) — STY filter qualifiers/default TUI.
- [NLM getNDCs](https://lhncbc.nlm.nih.gov/RxNav/APIs/api-RxNorm.getNDCs.html) — current curated normalized NDC scope.

**Examples in existing code:**

- `crates/cli/src/rxnorm.rs`, `rxnorm/reader.rs` — checksummed streaming activation.
- `crates/backend/src/store/native_valueset.rs` — native predicates/set algebra.
- `crates/backend/src/readiness.rs` — bounded schema/source metadata checks.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add and run regression tests

**What to add/implement:**

- Test all source combinations, loaded-empty versus unavailable, exact raw field preservation, malformed columns/UTF-8/identifiers/references, STY coverage and normalized NDC eligibility.
- Verify checksum/count/version/cohort changes, unchanged legacy fingerprint, path independence, immutable conflicts, dry run, rollback, cancellation and concurrent activation.
- Verify default/qualified STY lists, NDC exact strings, source-aware lookup/capabilities, no source/suppressed/unsupported attachment leakage, versions, mixed intersections/exclusions, deterministic filtered totals/pages, freshness and atomic failures.
- Preserve every existing assertion and provider limit. Test readiness/provenance constraints, missing/disabled tracking and lossless downgrade refusal.

**Files to edit/create:**

- `crates/cli/src/rxnorm/auxiliary_tests.rs`, `rxnorm/tests.rs` — importer/cohort scenarios.
- `crates/backend/src/rxnorm_auxiliary_tests.rs`, `rxnorm_provider_tests.rs` — public API/provider scenarios.
- `crates/backend/src/readiness_tests.rs`, `crates/core/src/rxnorm.rs` — readiness and filter grammar.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
```

### Stage 3: Verify integrated delivery and document support

**What to add/implement:**

- Run workspace/Clippy/format/independent-core/harness checks and actual CLI-to-HTTP smoke using generated complete-format files in a disposable database.
- Retain raw fixtures, failed attempts, logs and source hashes under `.benchmarks/rxnorm-auxiliary-20261106/`. Do not claim clinical content, ecosystem certification or performance/composite progress.
- Update report, README, requirements, product plan and handoff with actual counts, source/migration requirements and remaining provider/history limits. Do not commit, push or deploy.

**Files to edit/create:**

- `docs/RXNORM_AUXILIARY_REPORT.md`, `README.md`, `docs/API_REQUIREMENTS.md`, `docs/NEXT_SESSION.md`, `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`.
- `vibe/rxnorm-auxiliary-plan-track.md` — verified stage status.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
git diff --check
```
