# LOINC primary Part links and SNOMED concept refsets

## Task

Add source-backed coded LOINC axes and explicitly declared simple concept reference sets for native SNOMED. Preserve immutable first-publication source cohorts, old fingerprints, one-operation snapshots, inactive semantics, bounded work and explicit unsupported knowledge.

## Plan structure

Implement storage/import and typed providers, add source/API regressions, then verify integrated generated-file CLI/HTTP behavior and update the handoff.

## Execution plan

### Stage 1: Implement atomic sources and typed query semantics

**What to add/implement:**

- Add optional complete same-version `primaryLinks` to LOINC manifests, raw eight-column storage and source metadata in migration `20261106000003_loinc_primary_links`.
- Preserve all Primary-file rows, including Radiology/DocumentOntology. Validate exact main-table references, five required axes plus conditional Method, matching raw names/property/type, unique eligible axes and bounded LP identifiers. Project six coded properties without altering raw labels or registering Parts as standalone concepts.
- Add closed `PrimaryAxis`, normalized ASCII LP identifiers and exact `=`/bounded `in`/`exists` filters. Check source availability before any result; explicit lookup axes require the chosen edition's file, while default/wildcard lookup remains usable. Keep primary enumeration scope and unsupported accessory operations.
- Add optional `simpleRefsets` to SNOMED manifests with one complete concept-only six-column Snapshot and one to sixty-four sorted declared refsets. Migration `20261106000004_snomed_simple_refsets` stores validated source context, declared empty/nonempty registries and raw UUID members.
- Validate exact headers/checksums/counts/release dates, known concept members, declared row scopes and active simple-type refset concepts through a bounded inferred path to `446609009`. Preserve inactive rows and allow active members referencing inactive concepts.
- Keep old absent/null source identities byte-identical. Add explicit projection/source identity only when declared; publish all source/projection/registry changes in the original transaction with existing advisory locks. Reject later same-version enrichment.
- Add SNOMED `concept in`, implicit `?fhir_vs=refset/<id>` and bounded ECL `^<id>`. Ordinary membership uses active member rows without implicitly activating concepts; ECL retains its active-concept substrate. Check every declared-source dependency before success. Global refset enumeration, nonconcept refsets, association maps and unsupported ECL selectors remain explicit errors.
- Extend actual capabilities, fresh bounded readiness and lossless downgrade guards for both sources. Preserve all existing work/admission/snapshot limits.

**Files to edit/create:**

- `crates/cli/src/loinc.rs`, `loinc/primary_links.rs`, `loinc/reader.rs` — source cohort and typed property projection.
- `crates/cli/src/snomed.rs`, `snomed/simple_refsets.rs` — concept-only Snapshot activation.
- `crates/core/src/loinc.rs`, `snomed.rs`, `snomed/ecl.rs` — closed axes/refset grammar and implicit definitions.
- `crates/backend/src/store/{native_valueset,snomed_ecl,implicit_membership,compose_loader,lookup,capabilities}.rs`, `api/operations/lookup.rs` — source-aware predicates, finite fallback protection and lookup context.
- `crates/backend/migrations/20261106000003_loinc_primary_links.{up,down}.sql`, `20261106000004_snomed_simple_refsets.{up,down}.sql`, `crates/backend/src/readiness.rs` — storage, provenance and operational guards.
- `benchmarks/loinc-primary-links.template.json`, `snomed-simple-refsets.template.json` — intentionally incomplete source templates.

**Framework/library documentation:**

- [LOINC primary linkages](https://loinc.org/kb/enriched-linkages-between-loinc-terms-and-loinc-parts) — exact columns, 5/6 cardinality, names and type/property mapping.
- [FHIR R4 LOINC](https://hl7.org/fhir/R4/loinc.html) — coded axes and source semantics.
- [SNOMED simple reference sets](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/reference-set-release-file-specification/5.2-reference-set-types/5.2.1-content-reference-sets/5.2.1.1-simple-reference-set) — six-column Snapshot/UUID model.
- [SNOMED active fields](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/release-types-packages-and-files/3.1-common-features-of-all-release-files/3.1.4-meaning-of-the-active-field), [FHIR R4 SNOMED](https://hl7.org/fhir/R4/snomedct.html) — member/concept status distinction and entry points.

**Examples in existing code:**

- `crates/cli/src/rxnorm/relationships.rs` — bounded immutable optional-source activation.
- `crates/backend/src/store/snomed_ecl.rs`, `native_valueset.rs` — typed AST predicates and independent guards.
- `crates/backend/src/readiness.rs` — bounded source metadata checks.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add importer, API and schema regressions

**What to add/implement:**

- Test all coded axes/types, optional Method, raw nonprimary retention, duplicate/missing coverage, strict CSV and aggregate projection budgets.
- Test declared empty refsets, member UUID/source fields, active/inactive distinctions, type proof, invalid references, duplicate Snapshot identities, source bounds and malformed input.
- Verify byte-identical legacy identities, path-independent idempotence, immutable conflicts, dry run, rollback, cancellation and concurrent activation for both sources.
- Verify source availability before empty/early-positive results, versions/instances/dates, qualifiers/syntax, totals/pages, mixed imports/exclusions, committed freshness and cache parity.
- Verify readiness descriptor/registration consistency, tracking drift and downgrade/publication races. Extend the independent core consumer.

**Files to edit/create:**

- `crates/cli/src/loinc/primary_link_tests.rs`, `snomed/simple_refset_tests.rs` — importer/source cases.
- `crates/backend/src/loinc_primary_link_tests.rs`, `snomed_refset_tests.rs`, `readiness_tests.rs` — public API/provider/schema cases.
- Core public parser tests and `tests/core-consumer/src/main.rs` — library boundary.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
```

### Stage 3: Verify integrated delivery and update documentation

**What to add/implement:**

- Run all checks below and real generated-file CLI-to-HTTP smoke on an owned temporary local database, preserving prior assertions.
- Retain raw fixtures, failed/final logs, source/binary hashes and source archive under `.benchmarks/loinc-parts-snomed-refsets-20261106/`.
- Update README, API requirements, product plan, handoff and `docs/LOINC_PARTS_SNOMED_REFSETS_REPORT.md` with actual support, migration needs, counts and remaining limits.
- Do not commit, push, deploy, download clinical sources or claim new comparative/composite results.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
git diff --check
```
