# Native SNOMED synonym expansion search

Later continuation: [bounded mixed-provider ValueSets](MIXED_VALUESET_REPORT.md) implements the finite/native relation bridge identified in this checkpoint. The search policy and historical validation below remain; no clinical performance is remeasured.

November 6, 2026. This increment continues the native-provider work in the [product plan](PRODUCT_PLAN.md). It extends supported search without changing release projections, content ingestion, code identity or the original benchmark assertions. No clinical archive or new comparative performance result is included.

## Supported behavior

Native `$expand` text filtering now matches any of:

- The concept code.
- The selected display, including explicit compose overrides and existing `displayLanguage` selection.
- An active RF2 synonym description (`900000000000013009`) in the selected release's configured language, with active preferred (`900000000000548007`) or acceptable (`900000000000549004`) membership in that release's configured dialect reference set.

Description and language membership must belong to the selected `code_system_id`. Another release, foreign language/dialect, inactive description/member, FSN-only description or unknown acceptability cannot independently produce a synonym match. Inactive concepts remain subject to compose and `activeOnly`; description activity does not override concept activity.

Search is a literal case-insensitive substring, not tokenization, ranking, fuzzy search, wildcard syntax or ECL. `%`, `_`, backslash and SQL-looking text have no special meaning. Both operands of native text matching now use PostgreSQL `lower`, so a source term is searchable with its own spelling even when Rust and PostgreSQL lowercase differently. Finite text matching retains Rust Unicode lowercase. This changes native text-search behavior, not the separate [stored/request-local code-identity boundary](REQUEST_CONTEXT_REPORT.md#remaining-unicode-comparison-boundary).

`displayLanguage` changes display selection; it does not select another RF2 search dialect. The selected display still participates independently. A matching synonym does not replace the returned display. Search does not imply that every source designation is returned: existing designation selectors and limits remain.

The predicate applies to direct implicit, stored canonical/version, instance and inline native expansion, including imported-set intersections and exclusions. Correlated `EXISTS` predicates retain one row per concept even when several descriptions or language members match. Filtered total, deterministic C-collation ordering, zero-count and offset behavior remain. Validation membership is not filtered by synonym text.

[FHIR R4 expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html) delegates text-filter interpretation to the server; the actual policy is published in `TerminologyCapabilities.expansion.textFilter`. [FHIR R4 SNOMED guidance](https://hl7.org/fhir/R4/snomedct.html) describes edition and language-reference-set context. This is a bounded provider policy, not unrestricted SNOMED search conformance.

## Implementation and operating limits

`crates/backend/src/store/native_valueset.rs::query_options` adds the synonym predicate to the existing expansion-only branch. Descriptions and dialect membership are read in the same fresh repeatable-read transaction as release selection, composition, total and page. No request-local definitions are published, no synonym cache or generation TTL is introduced, and no full release is copied into Rust.

Existing `(code_system_id, concept_code)` description and `(code_system_id, description_id)` language-member indexes support correlated probes. Broad expansions can still probe many concepts and descriptions; this increment does not establish clinical search throughput or a new memory bound. The existing common admission, native application concurrency, SQL/native deadlines, hierarchy traversal and response-page limits remain unchanged. Database-side cancellation/draining limitations from [the native-provider report](SNOMED_VALUESET_REPORT.md) remain.

No migration, dependency update or RF2 projection revision is needed. All existing migrations through `20261104000006_supplements` are still required. Rebuild/restart the backend normally; production builds retain the frontend-before-RustEmbed requirement.

## Validation

The final workspace run passes **214 Rust/PostgreSQL tests, zero ignored**: 125 backend, 15 CLI, 63 core and 11 shared-ingestion tests. Eight new native-search scenarios cover the acceptance/rejection matrix, duplicate descriptions/language memberships, literal metacharacters and SQL-looking text, cross-release isolation, imported intersections/exclusions, exact totals/pages, commit/rollback freshness, release language/dialect changes, retention parity, lifecycle/display-language independence and metadata policy. Native LOINC tests retain Unicode display and literal/code search while rejecting RF2 synonym bleed, including an intentionally malformed cross-provider source fixture.

Workspace Clippy across all targets/features with warnings denied, formatting, the standalone locked core consumer/dependency guard, **38 harness tests / 40,251 assertions**, four ecosystem-adapter tests and changed/new-file whitespace checks pass. Original assertions remain intact. The [tracker](../vibe/native-search-plan-track.md) records every stage; raw logs and local-only semantic smoke output are under `.benchmarks/native-search-20261106/`. Final logs are `workspace-final-214.log`, `clippy-final-214.log`, `targeted-tests-eight.log`, `core-consumer.log`, `core-dependencies.txt`, `harness-final.log`, `adapter-final.log` and `local-gates-summary.json`.

The unchanged seven local FHIR gates pass in both retention modes on a temporary **debug** backend, with no load windows. Public inventory totals stay at 1,065 CodeSystems, 1,317 ValueSets and 81 ConceptMaps before/after. The runner terminates both temporary processes. This is API regression evidence, not a new embedded production/browser build or performance comparison.

Toolchain: Rust/Cargo 1.99.1, Bun 1.4.4, local PostgreSQL 17.10. Tests use isolated disposable schemas in the existing loopback database. The standalone core consumer and its no-transport/no-SQL dependency guard remain independent of the backend.

Reproduce from the repository root against a local disposable database:

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
```

The final local Cargo commands use `--offline` after fetching both locked dependency sets. The independent consumer has its own lockfile; a workspace fetch alone did not populate every consumer dependency. No lockfile is changed.

The first targeted run failed because its new positive baseline supplied an invalid empty FHIR `valueString`. The fixture now omits `filter` for the unfiltered positive and independently asserts HTTP 400 for the explicit empty primitive. No admission or product assertion was weakened; the failing log is retained.

## Remaining work

- Mixed arbitrary finite/native composition still needs a bounded relation bridge and shared identity semantics; removing its dispatch guard is not a solution.
- Whole-grammar UCUM enumeration, LOINC accessories/hierarchy, native RxNorm, SNOMED ECL/refsets and full licensed clinical content remain incomplete.
- The SQL/standalone Unicode code-identity discrepancy remains open. A shared versioned policy must cover ingestion, fallback queries/indexes, retained plans, request-local resources, supplements and collision rejection before changing it.
- Full-release synonym search cost must be measured on matched complete clinical content. No throughput, general-mode ecosystem pass count or original HealthSamurai composite is remeasured here. Preserve the historical 110/689 audit, 2/20 clinical preflight and all reported timing regressions as historical evidence, not a new run.

No commit, push, publication or deployment is performed.
