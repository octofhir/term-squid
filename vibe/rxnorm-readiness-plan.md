# RxNorm, readiness and UCUM tools

## Task

Add a checksummed local full-monthly RxNorm atom importer and a conservative current-active native provider. Deliver explicit/scoped mixed ValueSets, real names/designations and source filters without claiming historical completeness. Add a fresh bounded readiness endpoint and reusable/CLI UCUM conversion and canonical-form tools without inventing new FHIR operations.

Preserve all preceding uncommitted native-search and mixed-provider work. No commit, push, publication, deployment, clinical archive download or comparative performance claim is included.

## Execution plan

### Stage 1: Implement three end-to-end features

**What to add/implement:**

- Add core RxNorm identity/version/filter primitives and migration `20261106000000_rxnorm_rrf` with release metadata, raw atoms, native-provider constraint and generation triggers.
- Add `import-rxnorm --manifest FILE [--dry-run]` with bounded UTF-8 RRF parsing, raw-byte SHA-256, exact counts, immutable versions, streaming staging/COPY and atomic activation. Retain every source atom/field; project only active normalized RXNORM concepts with an unambiguous primary name.
- Add known-active point reads, real designations, typed properties, literal atom search, source/term-type filters and bounded mixed-provider ValueSets. Accept official `/vs` membership for known active codes; require an explicit active scope for enumeration. Missing/suppressed/archived knowledge remains an error, not a conclusive invalid-code result. Do not manufacture subsumption.
- Add `/ready`: fresh read-only database/schema/migration/generation/native-registration checks, bounded timeout and probe admission, generic unavailable response without database details. Keep `/health` database-free and readiness independent of clinical corpus completeness.
- Add bounded `UcumProvider::convert` and `canonical`, preserving exact canonical factor/offset strings. Add database-free JSON CLI commands for both. Keep the six-operation FHIR contract unchanged.

**Files to edit/create:**

- `crates/core/src/rxnorm.rs`, `lib.rs`, `ucum.rs` — native identity/filter grammar and unit tools.
- `crates/cli/src/rxnorm.rs`, `rxnorm/reader.rs`, `main.rs` — streaming import and commands.
- `crates/backend/migrations/20261106000000_rxnorm_rrf.{up,down}.sql` — native source storage and safe rollback.
- `crates/backend/src/store/{native_valueset,postgres,concepts,lookup,validation,designations,expansion,capabilities,transient,compose_loader,selection,hierarchy,translation}.rs`, `concept_resolution.sql`, `subsumption.sql` — provider routing, partial knowledge and source labels.
- `crates/backend/src/readiness.rs`, `main.rs` — isolated readiness router and application wiring.
- `benchmarks/rxnorm-rrf.template.json` — intentionally incomplete operator manifest.

**Framework/Library documentation:**

- [FHIR R4 RxNorm](https://hl7.org/fhir/R4/rxnorm.html) — canonical identity, versions, display and filter meanings.
- [NLM RRF format](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html), [TTY definitions](https://www.nlm.nih.gov/research/umls/rxnorm/docs/appendix5.html), [status meanings](https://lhncbc.nlm.nih.gov/RxNav/APIs/api-RxNorm.getRxcuiHistoryStatus.html).
- Locked local `octofhir-ucum-0.9.3/src/lib.rs` — conversion/canonical APIs; locked SQLx 0.9 migrator API for readiness.

**Examples in existing code:**

- `crates/cli/src/loinc.rs`, `loinc/reader.rs`, `snomed.rs` — streaming source/activation workflow.
- `crates/backend/src/store/finite_bridge.rs`, `native_valueset.rs` — snapshot, admission and provider boundaries.
- `crates/backend/src/store/designations.rs` — bounded labels.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add and run independent regression tests

**What to add/implement:**

- Test importer input bounds, strict RRF/UTF-8, all-field preservation, source/atom identity, primary-name ambiguity, active/suppressed/source semantics, checksum/count conflicts, dry run, immutable/idempotent activation, concurrency, cancellation and rollback.
- Test public RxNorm point reads, partial history errors, scoped/implicit/mixed sets, source/TTY filters, aliases/display validation, versions/dates, supplements, temporary override guard and cache freshness.
- Test readiness healthy/unavailable paths, migration/checksum/guard/trigger/native-registration drift, cancellation, pool/probe limits and recovery without mutations or secrets.
- Test UCUM prefixed/compound/temperature/nonlinear conversion, incompatible/invalid/nonfinite/oversized input, canonical decimal factors/offsets and database-free CLI operation.

**Files to edit/create:**

- `crates/cli/src/rxnorm/tests.rs`, `crates/core/src/rxnorm.rs`, `ucum.rs` — parser/importer/core scenarios.
- `crates/backend/src/rxnorm_provider_tests.rs`, `tests.rs` — public provider scenarios.
- `crates/backend/src/readiness.rs` — isolated readiness tests.

**Examples in existing code:**

- `crates/backend/src/loinc_provider_tests.rs`, `mixed_valueset_tests.rs`, `crates/cli/src/loinc/tests.rs`.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
```

### Stage 3: Verify integrated delivery and update documentation

**What to add/implement:**

- Run all workspace/Clippy/format/independent-core/harness checks and a real local CLI-import/HTTP smoke using generated complete-format source in a disposable schema/database. Do not relabel generated data as clinical releases.
- Document source scope, manifest/command examples, migrations, readiness semantics and operational/provider limits. Update the product plan and handoff.
- Keep raw data/logs and measurements under `.benchmarks/rxnorm-ready-20261106/`; retain failed diagnostics and original benchmark assertions.

**Files to edit/create:**

- `docs/RXNORM_READINESS_REPORT.md`, `README.md`, `docs/API_REQUIREMENTS.md`, `docs/NEXT_SESSION.md`, `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`.
- `vibe/rxnorm-readiness-plan-track.md` — verified stage status.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
git diff --check
```
