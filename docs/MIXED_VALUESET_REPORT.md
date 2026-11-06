# Bounded mixed-provider ValueSets

November 6, 2026. This increment follows [native synonym search](NATIVE_SEARCH_REPORT.md) and implements the mixed-provider composition step in the [product plan](PRODUCT_PLAN.md). It adds functionality, not clinical content or a new throughput/composite result.

## Supported composition

One ValueSet can now combine already supported native SNOMED/LOINC selections with complete finite CodeSystems and explicit bundled UCUM lists. Stored canonical/version, instance, inline, imported, container-scoped and request-local definitions use the same provider boundary. Native sources remain native projections: they are never interpreted as empty finite CodeSystems or copied into a release-sized Rust plan.

Finite leaves reuse the existing engine for property/regex/hierarchy filters, case policy, lifecycle, explicit display overrides and supplements. Explicit UCUM expressions still use the real bundled grammar, with its existing version and enumeration restrictions. Native SNOMED/LOINC operators and completeness constraints do not expand in this increment.

The native graph compiler combines finite relations and native selections, counts the complete filtered result and emits the requested deterministic page. [FHIR R4 composition](https://hl7.org/fhir/R4/valueset.html#compositions) defines cumulative includes, intersections within a selection, exclusions and system/version/code uniqueness. The implementation preserves:

- Union deduplication by actual system/business-version/canonical-source-code identity and last-include display precedence.
- Imported intersections, including system-plus-import selection. Display comes from that selection's system, or its first imported set.
- Exact-edition exclusions, unversioned concept/filter/import exclusions across editions, and whole unversioned exclusions of the entire system, including old codes absent from the latest release.
- Independent system and ValueSet versions; version directives and source/date selection remain on the operation snapshot.
- Original source spelling. Finite membership resolves case-insensitive queries in Rust; set joins compare already canonical source codes, not database-lowercased strings. An exclusion is not prefiltered by the query's spelling or the include's case policy.

Temporary finite resources override stored content according to the existing request-context resolver, including when URL/business-version match. Virtual SQL metadata identities are operation-owned and keyed by actual canonical edition, so versioned/unversioned aliases deduplicate without consulting persisted concept rows. They never become published resources or public provenance. The next request without that context still sees stored content.

## Rich results and knowledge

Finite rows carry inactive, abstract and post-coordination state, membership candidacy and finite text-match results. Lifecycle/UI/post-coordination/system/text filters run before total and paging. Finite text search retains Rust Unicode lowercase; native code/display/synonym search retains its documented PostgreSQL policy. This is not a new universal Unicode identity policy.

Page-local finite metadata preserves source primary display, enriched designations and structural parents. Rich expansion uses it for designation selectors and nested output rather than re-resolving temporary or UCUM identities in the database. Validation uses the same source labels, abstract policy and compact complete/partial source knowledge. Known finite codes outside a set remain distinguishable from unknown codes; native knowledge and partial UCUM projections do not falsely become complete. ConceptMap scope membership also uses the mixed boundary.

The internal single-selection ValueSet has no imports and is never exposed. User imports stay in the original graph, and actual selected CodeSystem, ValueSet and supplement provenance remains. Mandatory supplement checks apply to all selected providers before returning any result.

## Snapshot, admission and cancellation

Root definitions, dependencies, finite source JSON/supplements, native projections, final count and page are read in one fresh repeatable-read, read-only transaction. There is no new global plan/result cache, TTL, temporary database table or write. The existing finite-only retained path is unchanged.

Additional aggregate finite-bridge limits apply to the whole mixed operation, not just its returned page:

| Bound | Limit |
| --- | ---: |
| Loaded finite source JSON, including repeated loads | 32 MiB |
| Finite source concepts, including repeated selections | 100,000 |
| Prepared finite relation rows across selections | 100,000 |
| Conservative finite rows/metadata/knowledge charge | 32 MiB |
| Designations per bridged selected concept | 256 |
| Distinct explicit bundled UCUM expressions across leaves | 1,024 |

The bridge charge includes compact source knowledge and conservatively charged serialized rows/labels. It is admission accounting, not a total Rust/SQL/RSS guarantee. Temporary decoding, core preparation and native database work have their existing independent bounds. A large source or repeated selection can therefore be rejected even for `count=0`, one requested row or an early matching native code. Oversized source JSON retains the existing loader's HTTP 400 error; finite work/metadata/bridge bounds return 422. No partial successful page or membership result is published.

Native definition/term/resource/depth/traversal limits, 64 common ValueSet requests, four native application work permits, fifteen-second native phase, ten-second statements and the conservative 32 MiB response-page charge are not raised. Finite preparation runs on the existing blocking CPU path and owns the same native work permit. Cancelling the awaiting operation does not restore CPU capacity while that preparation still executes. One bounded synchronous engine call cannot be forcibly interrupted; it eventually finishes and releases its permit. SQLx database-draining/cancellation limitations remain separate and unchanged.

## Implementation and upgrade

- `store/compose_loader.rs` adds an internal single-selection loader and source-concept accounting.
- `store/finite_bridge.rs` owns finite preparation, aggregate bounds, canonical identities, rows, source knowledge and page metadata.
- `store/native_valueset.rs` compiles finite JSON relations and selected metadata alongside native projections, preserving native dispatch guards and exact set semantics.
- `store/value_set.rs`, `expansion.rs`, `validation.rs`, `translation.rs` and `postgres.rs` carry page metadata and owned CPU admission through the existing operation adapters.

No migration, dependency or core-library API change is required. Apply existing migrations through `20261104000006_supplements`. Rebuild/restart normally; production builds still require the frontend before RustEmbed. No commit, push, publication or deployment is performed.

## Validation

The full workspace passes **229 Rust/PostgreSQL tests, zero ignored**: 140 backend, 15 CLI, 63 core and 11 shared-ingestion tests. All earlier tests, including the eight native-search scenarios, remain enabled. Fifteen new mixed-provider scenarios cover:

- Four-provider unions, actual edition aliases, last-display deduplication, imported/system intersections and every exclusion scope.
- Temporary/contained isolation, finite filter semantics, exact Unicode source identity and mixed case policies.
- Supplements, designations, localization, abstract/inactive/UI/post-coordination options, nested results and CodeableConcept diagnostics.
- UCUM expressions/partial knowledge, unavailable/incomplete/unsupported inputs, retention parity, commit/rollback and concurrent snapshot consistency.
- Whole-operation byte/concept/repeated-source/row/metadata/selection admission, including zero-count and early-positive requests; 256 designation success versus 257 rejection.
- Deterministic cancellation through the public store operation: a task-local test pause observes work capacity 4 → 3, cancellation leaves it at 3 until the blocking worker is released, then capacity returns to 4 and a real mixed query succeeds.

Workspace Clippy across all targets/features with warnings denied, formatting, the locked standalone core consumer/no-transport/no-SQL dependency guard, **38 harness tests / 40,251 assertions** and four ecosystem-adapter tests pass. No benchmark assertion, pool, scorer or clinical corpus is changed.

Two focused cross-reviews found no confirmed blocker in canonical identity/set algebra, source knowledge/supplement provenance, rich adapters, atomic admission or cancellation ownership. They are code-inspection evidence, not an additional conformance certification.

Both unchanged seven-case local FHIR gate profiles pass on a temporary **debug** backend, retention on/off, without load windows. Public inventory totals remain 1,065 CodeSystems, 1,317 ValueSets and 81 ConceptMaps before/after. The runner stops its temporary processes. This is not a newly built/verified embedded production UI, browser suite, full ecosystem audit or matched performance cohort.

Raw evidence is under `.benchmarks/mixed-valueset-20261106/`: `targeted-fifteen.log`, `workspace-final.log`, `clippy-final.log`, `check-final.log`, `core-consumer.log`, `core-dependencies.txt`, `harness.log`, `adapter.log` and `local-gates-summary.json`. The first new bound fixture expected 422 for the existing 400 raw-source loader error; the corrected test asserts that exact contract and still forbids partial success. Its failed output is retained.

Toolchain: Rust/Cargo 1.99.1, Bun 1.4.4 and loopback PostgreSQL 17.10. Reproduce from the repository root with a local disposable database:

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
```

The local final Cargo runs add `--offline` after both locked dependency sets are available.

## Remaining work

Mixed composition is bounded by each participating provider's existing knowledge. It does not add whole-grammar UCUM enumeration, implicit UCUM conventions, SNOMED ECL/refsets, LOINC accessories/hierarchy, native RxNorm or complete licensed clinical releases. Oversized finite leaves do not silently switch to a partial SQL fallback.

The [SQL/Rust Unicode code-identity boundary](REQUEST_CONTEXT_REPORT.md#remaining-unicode-comparison-boundary) still requires a shared versioned policy, collision checks and projection/index compatibility. No equality equivalence beyond the existing engine/native contracts is claimed.

Full clinical ingestion, the original twenty preflights and matched local Termbox/FHIRsmith measurements remain necessary before a clinical composite claim. Historical 2/20 preflight, 110/689 ecosystem coverage and all recorded performance regressions remain historical evidence; they are not rerun or improved by these generated tests.
