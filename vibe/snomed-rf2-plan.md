# Native SNOMED RF2 Snapshot import

## Task

Import complete, locally supplied RF2 Concept/Description/inferred Relationship/Language Snapshots without materializing the clinical release as a FHIR JSON document or transitive closure. Preserve source hashes, edition/version/dialect, all terms and relationships, and atomic visibility. Support projected lookup/validation and bounded native subsumption; unsupported native compose remains an explicit error.

## Execution plan

### Stage 1: Native storage and read adapters

**What to add/implement:**

- Add nullable `native_provider` and version-scoped release/description/relationship/language tables, indexes, foreign keys and generation triggers in reversible migration `20261103000001_snomed_rf2`.
- Preserve FHIR metadata `content=not-present`; resolve complete native-provider knowledge separately in concept reads. Force native hierarchies through a bounded, single-statement active inferred is-a traversal rather than interpreting an empty closure as no relationship.
- Keep generic closure reads and all prior stored semantics. Reject native finite-compose preparation until its provider is implemented, rather than returning an empty expansion.

**Files to edit/create:**

- `crates/backend/migrations/20261103000001_snomed_rf2.{up,down}.sql`.
- `crates/backend/src/store/{postgres.rs,concepts.rs,hierarchy.rs,subsumption.rs,subsumption.sql,mod.rs}`.

**Examples in existing code:**

- `store/postgres.rs::subsumption_row`, `store/concepts.rs`, `20261103000000_hierarchy_generation.up.sql`.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --workspace --offline`

### Stage 2: Native read regression tests

**What to add/implement:**

- Test canonical/version/instance concept knowledge, all four subsumption outcomes, inactive/non-is-a edges, missing codes and traversal bounds.
- Test generation freshness after native edge mutations, rollback and deletion; assert native compose is unsupported, not empty. Keep earlier tests unchanged.

**Files to edit/create:**

- `crates/backend/src/tests.rs` — isolated-schema public HTTP/store tests.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace --offline -- --include-ignored`

### Stage 3: Streaming RF2 importer

**What to add/implement:**

- Add `import-snomed --manifest FILE [--dry-run]`. Require an attested complete merged Snapshot, explicit release/module/dialect/language and expected SHA-256 for each local source; do not download or extract clinical archives.
- Stream bounded validated UTF-8 TSV records through bounded COPY buffers into transaction-local staging. Reject malformed headers/rows, unpublished/future dates, duplicates, unsupported relationship characteristics and checksum mismatch.
- Validate references, selected dialect/FSNs, ambiguous preferred synonyms and inferred hierarchy cycles/depth. Preserve all input descriptions/language members/relationship groups and lifecycle fields.
- Publish metadata, projections and native rows in one transaction; immutable identical imports are idempotent, conflicting versions are refused. Dry-run rolls back all persistent state. No per-concept SQL or full-release JSON/closure materialization.

**Files to edit/create:**

- `crates/cli/src/{main.rs,snomed.rs,snomed/reader.rs}`, `crates/cli/Cargo.toml`, `Cargo.lock`.

**Framework/library documentation:**

- SNOMED International RF2 Concept, Description, Relationship and Language Reference Set specifications; SNOMED CT URI Standard.
- SQLx 0.8 PostgreSQL `copy_in_raw` API from the locked source.

**Examples in existing code:**

- `crates/cli/src/terminology.rs` — transactional projection import; do not reuse its finite-plan limits for native releases.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo check --workspace --offline`

### Stage 4: Importer tests

**What to add/implement:**

- Generate original synthetic RF2 files in temporary directories; do not embed licensed clinical content or copy benchmark code pools.
- Test row validation/checksums/limits, dialect display selection, native projection/provenance, idempotence/conflict, dry-run, cancellation/error rollback, invalid references/cycles/ambiguous preferences, and versions.

**Files to edit/create:**

- `crates/cli/src/snomed/tests.rs` — public importer tests and generated fixtures.

**Verification commands:**

- `TERM_SQUID_TEST_DATABASE_URL=postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace --offline -- --include-ignored`
- `CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --offline --all-targets -- -D warnings`

### Stage 5: Local release smoke and handoff

**What to add/implement:**

- Build release CLI/server, apply the actual migration, and exercise native import plus HTTP lookup/subsumption in an isolated synthetic schema. Preserve existing benchmark data and UI.
- Expose repository migrations through `term-squid-cli migrate` and use it in `bench-migrate`, so a new device does not require the local ignored migration helper or a separately installed SQLx CLI.
- Re-run unchanged local/upstream preflights. Record that complete licensed clinical data and native ValueSet/property/map behavior remain unmeasured/incomplete; do not fabricate a composite improvement.
- Document the manifest, limits, snapshot scope, migrations/restart, activation/freshness and remaining work. Update product plan/handoff and tracker.

**Files to edit/create:**

- `docs/SNOMED_RF2_REPORT.md`, `docs/PRODUCT_PLAN.md`, `docs/DEVELOPMENT_HANDOFF.md`, `benchmarks/README.md`.

**Verification commands:**

- `CARGO_TARGET_DIR=.benchmarks/target cargo build --release --offline --bin backend --bin term-squid-cli`
- Local CLI/HTTP smoke and both unchanged preflight suites.
- `cargo fmt --all -- --check`
- `bun test benchmarks`
- `git diff --check`
