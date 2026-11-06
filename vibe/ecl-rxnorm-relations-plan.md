# Bounded SNOMED ECL and RxNorm relationships

## Task

Add active-concept SNOMED ECL set queries and source-backed RxNorm relationship filters through the existing R4 operations. Preserve one-operation snapshots, immutable import cohorts, explicit unsupported knowledge, source direction, bounded work and all preceding changes.

## Plan structure

Implement typed/core and source/storage/provider changes, add regression coverage, then verify the integrated CLI/API delivery and update the handoff.

## Execution plan

### Stage 1: Implement typed queries and atomic relationship sources

**What to add/implement:**

- Add a bounded, dependency-free ECL AST/parser: scalar SCTIDs, optional terms, wildcard, transitive/direct ancestor/descendant operators, homogeneous conjunction/disjunction, binary exclusion and parentheses. Reject ambiguous mixed operators, unsupported refinements/refsets/history and malformed input.
- Support `constraint =` and strict once-decoded `?fhir_vs=ecl/` URLs. Keep existing whole/isa membership optimization. Evaluate ECL only on active inferred concept graphs; validate every focus before success and count all walks against existing operation limits.
- Compile typed expressions into bound SQL set predicates with complete traversal guards, preserving totals/pages, membership, imported/excluded/mixed sets and edition selection.
- Add optional full-monthly `relationships` manifest source for RXNREL.RRF, raw sixteen-column storage, source availability/provenance and generation tracking in migration `20261106000002_rxnorm_relationships`.
- Parse and validate declared relationship sources in the same publication transaction. Preserve raw rows and original no-relationship metadata/fingerprints; require source selection at first publication.
- Add standard R4 REL/RELA `=`/`in` filters with explicit CUI/AUI anchors, official endpoint direction and closed SQL field inventory. Validate anchor existence using raw CONSO atoms, not only active normalized concepts. Missing source remains unsupported; loaded-empty is known absence.
- Advertise only actual supported query/source capabilities. Extend fresh readiness and safe downgrade without scanning clinical corpora.

**Files to edit/create:**

- `crates/core/src/snomed.rs`, `snomed/ecl.rs` — typed ECL grammar and implicit selection.
- `crates/backend/src/store/snomed_ecl.rs`, `native_valueset.rs`, `implicit_membership.rs`, `capabilities.rs` — SQL evaluation and source-aware metadata.
- `crates/core/src/rxnorm.rs` — typed relationship grammar.
- `crates/cli/src/rxnorm.rs`, `rxnorm/relationships.rs` — checksummed same-cohort import.
- `crates/backend/migrations/20261106000002_rxnorm_relationships.{up,down}.sql`, `crates/backend/src/readiness.rs` — raw storage and operational guards.
- `benchmarks/rxnorm-relationships.template.json` — intentionally incomplete manifest.

**Framework/library documentation:**

- [FHIR R4 SNOMED](https://hl7.org/fhir/R4/snomedct.html) — constraint filter and implicit URLs.
- [SNOMED ECL syntax](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language/design/5-syntax-specification) — active substrate, operators and mandatory disambiguation.
- [FHIR R4 RxNorm](https://hl7.org/fhir/R4/rxnorm.html) — REL/RELA CUI/AUI filtering.
- [NLM technical documentation](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html), [relationship inventory](https://www.nlm.nih.gov/research/umls/rxnorm/docs/appendix1.html) — exact columns and endpoint direction.

**Examples in existing code:**

- `crates/backend/src/store/native_valueset.rs` — bounded recursive walks and set composition.
- `crates/cli/src/rxnorm/auxiliary.rs` — same-cohort streaming activation.
- `crates/backend/src/readiness.rs` — fresh structural probes.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add regression and atomicity tests

**What to add/implement:**

- Test every supported ECL operator, active graph semantics, diamond/cycle behavior, labels, invalid syntax, bounds, editions, all invocation forms, mixed composition, exact pages, freshness, cancellation and non-partial guards.
- Test RXNREL exact raw preservation, CUI/AUI attachments, malformed records/checksums/counts, immutable fingerprints/cohorts, loaded-empty versus missing, dry run/rollback/cancellation and atomic activation.
- Test standard relationship direction, raw source-only anchors, CUI/AUI list grammar, no suppression leakage, unique results, explicit unknown knowledge and source-dependent capabilities/readiness/downgrade.
- Extend the independent core consumer without database/async/HTTP dependencies.

**Files to edit/create:**

- `crates/backend/src/snomed_ecl_tests.rs`, `rxnorm_relationship_tests.rs`, `readiness_tests.rs`, `tests.rs` — API/provider/database regressions.
- `crates/cli/src/rxnorm/relationship_tests.rs` — importer regressions.
- `crates/core/src/snomed/ecl.rs`, `rxnorm.rs`, `tests/core-consumer/src/main.rs` — public parser/consumer checks.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
```

### Stage 3: Verify integrated delivery and update handoff

**What to add/implement:**

- Run all checks below and an actual generated CLI-to-HTTP smoke in an owned disposable local database.
- Keep initial/final logs, fixtures and source hashes under `.benchmarks/ecl-rxnorm-relations-20261106/`.
- Update README, API requirements, product plan, handoff and `docs/ECL_RXNORM_RELATIONS_REPORT.md` with actual support, migration requirements, counts and remaining limits.
- Preserve all earlier work. Do not commit, push, deploy, download clinical content or claim new comparative/composite results.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
git diff --check
```
