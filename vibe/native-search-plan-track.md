# Progress tracker for native-search-plan.md

Status format: [ ] - pending, [X] - completed.

## Execution stages

[X] Stage 1: Extend the native text predicate
[X] Stage 2: Add native search regression tests
[X] Stage 3: Describe the supported search policy
[X] Stage 4: Verify and record the increment

## Notes

- November 6, 2026: clean starting tree. Standalone Git; no rebase, commit, push or deployment. Existing PostgreSQL container and main corpus are preserved; tests use disposable schemas.
- Complete clinical archives are unavailable in this checkout. Generated native tests do not establish clinical coverage, performance or a composite score.
- Unicode code-identity consistency remains separate: PostgreSQL locale-based lowercase and Rust Unicode lowercase are not interchangeable. No identity policy or projection semantics are changed here.
- Stage 1: `cargo check --locked --offline --workspace` passes. Independent SQL review found no correctness blocker; existing indexed correlations, snapshot and deadlines remain. Full-release search cost is not measured.
- Stage 2: six new PostgreSQL/HTTP tests pass, zero ignored. The first run exposed an invalid empty FHIR `valueString` in the new fixture; the test now omits the filter for its positive unfiltered baseline. Strict parameter admission is unchanged, and the failed run is retained. Existing 206 workspace tests also pass before the new test module is compiled.
- Stage 3: metadata and API requirements describe actual synonym scope, literal matching and native/finite text case policies. Six tests pass again with metadata assertions and an explicit empty-filter rejection check.
- Stage 4: expanded the new module to eight passing tests with duplicate language members, release-context commit/rollback freshness and LOINC literal/Unicode/provider isolation. Final workspace run passes 214 tests (125 backend, 15 CLI, 63 core, 11 ingestion), zero ignored; all-target/all-feature Clippy with warnings denied, formatting, locked independent consumer/dependency guard, 38 harness tests / 40,251 assertions, four ecosystem-adapter tests and whitespace checks pass.
- Seven unchanged local API gates pass in both retention modes on a temporary debug backend; public inventory totals are unchanged and both processes stop. No load, browser/production build, official general audit or clinical composite is rerun. Reports and continuation files record this distinction.
- Final raw evidence stays under `.benchmarks/native-search-20261106/`; see `docs/NATIVE_SEARCH_REPORT.md`. No commit, push or deployment.
