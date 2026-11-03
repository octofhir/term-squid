# Helios HTS feature audit

Read-only source audit, November 3, 2026. Reference: [HeliosSoftware/hfs](https://github.com/HeliosSoftware/hfs/tree/96a4922a424d870ad6e41702a02e3c72ba2949c8/crates/hts), revision `96a4922a424d870ad6e41702a02e3c72ba2949c8`. No image was deployed or load-tested. Source evidence does not establish clinical conformance or runtime performance.

HTS currently has a broader feature surface than term-squid. Our measured synthetic speedups and cross-process freshness work do not establish feature superiority. Composite-score work takes priority; reuse the relevant input/content improvements below rather than implementing every HTS endpoint first.

## Evidence and opportunities

Paths below are relative to the pinned repository's `crates/hts/` unless stated otherwise.

| Area | HTS source evidence | term-squid / action |
|---|---|---|
| FHIR releases | `Cargo.toml`, `tests/concept_map_ops.rs`: R4 default, R4B/R5/R6 features; release-specific tests | Verified R4 subset only; do not advertise R5 from routes alone |
| Operation inputs | `src/operations/params.rs`, `validate_code.rs`: Coding/CodeableConcept, version context, inline resources | The subsequent inline increment adds bounded `Parameter.resource` handling; richer Coding/CodeableConcept normalization and conflicts remain |
| Lookup | `src/operations/lookup.rs`, `tests/coding_properties.rs`: typed properties, designations/language, supplements | Our projected properties are lossy and designation handling is incomplete. Preserve types/language through importer, engine, cache and adapter |
| Validation | `src/operations/validate_code.rs`: display/language/lifecycle/supplement handling and diagnostics | Exact-display comparison is not equivalent coverage; add a pinned requirements matrix and ecosystem cases |
| API breadth | `src/server.rs`: batch validation, Bundle batch/transaction, CRUD/history/search | Do not prioritize public authoring/history over composite-critical reads; keep first-release scope |
| Closure | `src/backends/sqlite/concept_map.rs`: pairwise results for the current request with echoed name/version | Not proof of persistent incremental `$closure`; persistent state remains a separate design |
| Import | `src/import/mod.rs`: FHIR bundles/packages, SNOMED, LOINC, UCUM, RxNorm and other importers; SQLite/PostgreSQL | Native RF2 Concept/Description/inferred Relationship/Language Snapshot import is now implemented; native LOINC/RxNorm and broader refsets remain missing. Importer presence alone is not semantic completeness |
| UCUM | `src/ucum_validate.rs`: `octofhir-ucum` grammar fallback with composed-unit tests | Canonical grammar lookup/validation now use octofhir-ucum 0.9.1, with tested Rust comparability. ValueSet integration and full conformance remain; this is not a common-units list |
| LOINC | `src/import/loinc_csv.rs`: synthetic-only importer fixtures, first-parent collapse, non-active status encoded in definition text | Clear opportunity: official release, all parents, typed axes/class/status, Parts, Answer Lists, Groups and languages, with atomic activation |
| SNOMED/ECL | `src/import/snomed_rf2.rs` and `src/ecl/mod.rs`: RF2 roles/language/maps, subset ECL evaluator restricted to SQLite | Native RF2 subset and bounded inferred subsumption now exist, but not implicit ValueSets/property filtering/ECL. Build the tested benchmark-required subset; do not claim full ECL |
| VCL | `src/vcl/mod.rs`: declared subset and explicit unsupported errors | Preserve explicit errors rather than silently broadening results |
| Freshness | `src/traits/caches.rs`: explicitly in-process cache invalidation contract | Our trigger generations and fresh observation batches already test separate-process writes, rollback and TRUNCATE. HTS cross-process guarantees were not established by this audit |
| Limits | CLI defaults: 10-MiB decompressed request, expansion 3,500, 30-second timeout; library expansion default differs | Retained-cache budgets are not whole-process memory limits; benchmark deployment limits explicitly |
| Ecosystem CI | Root `.github/workflows/hts-ig-conformance.yml`: validator/suite fetched as latest, private ignored cases/skips | Add a pinned reproducible gate; skips are not passes. Test cache on/off and match metadata to actual behavior |

## Follow-up order

1. **Composite-critical input support:** bounded inline roots are now implemented (see [report](INLINE_VALUESET_REPORT.md)); complete richer normalization in `crates/backend/src/api/parameters.rs`, `api/operations/expand.rs` and `validate.rs`; evaluate bounded inline ValueSets through `crates/core/src/lib.rs` and the shared store. Test malformed/ambiguous inputs, include/exclude, nested dependencies, limits and update freshness. Resolve the upstream `target` compatibility discrepancy without weakening R4 semantics.
2. **Clinical corpus:** the [native RF2 increment](SNOMED_RF2_REPORT.md) provides the first Snapshot loader. Continue extending `crates/cli/src/terminology.rs` and the storage/provider contracts for licensed RF2, LOINC and RxNorm releases, checksums, reproducible projection, atomic activation and rollback. Support SNOMED implicit sets, hierarchy and properties needed by the unchanged benchmark pools. Never substitute only the sampled codes for the full release.
3. **Correct rich reads:** typed properties, Coding/version normalization, designations/language, lifecycle and supplements end-to-end, with pinned ecosystem resources and capabilities that describe the tested subset.
4. **Content differentiation:** extend the [implemented UCUM grammar provider](UCUM_PROVIDER_REPORT.md) to finite ValueSet workflows and build an official LOINC importer that preserves all parents and typed attributes rather than the lossy behaviors above.
5. **Proof:** publish same-environment composite/coverage against local Termbox/FHIRsmith; if adding HTS to performance comparisons, pin its image digest/source and content first. Implement broader authoring only when product scope requires it.

The Docker documentation describes `ghcr.io/heliossoftware/hts`, amd64/arm64 images, port 8090, a bootstrap directory and non-root execution. Availability, digest and runtime behavior were not verified; floating `latest` is unsuitable for a reproducible comparison.

The repository source is MIT-licensed (`LICENSE.md`); preserve its notice if code is reused. Clinical content has separate terms described in `crates/hts/README.md` and the benchmark data documentation. No HTS code or licensed content was copied into term-squid in this increment.
