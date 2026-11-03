# Legacy R4 core package compatibility

## Scope

`hl7.fhir.r4.core#4.0.1` contains six legacy v2 CodeSystems whose `url` embeds a business version while `version` contains the table number. For example, `http://terminology.hl7.org/CodeSystem/v2-0006|2.4` has `version: "0006"`. Six ValueSet compose selections reference these pipe-versioned system URLs.

The strict canonical parser correctly rejects the contradictory versions. During reference setup, the CLI therefore reported six import errors and rolled back the entire package transaction. No partial package became active. FHIRsmith normalizes these published legacy resources on load; see its pinned [package loader](https://github.com/HealthIntersections/FHIRsmith/blob/25a41f87711507789b361480ee4efae42df504d4/library/package-manager.js#L930).

## Import boundary

The CLI package extractor now applies a narrow compatibility adapter only when package metadata identifies `hl7.fhir.r4.core` version `4.0.1`:

- Known table/version pairs are `0006` at `2.1`/`2.4`, `0360` at `2.3.1`/`2.7`, and `0391` at `2.4`/`2.6`.
- A CodeSystem is normalized only when its current version equals the table number. Its stored URL becomes the base URL and its business version comes from the URL suffix.
- A matching ValueSet include/exclude system reference is split into `system` and `version` only when no explicit version is present.
- Other packages, package versions, tables, business versions and explicit conflicts remain unchanged. Expansion-only ValueSets do not gain an empty compose.

The downloaded archive stays unchanged; stored FHIR resource JSON contains the normalized URL/version. The CLI logs the number of corrected definitions/references. The engine's `Canonical::parse` and operation version-conflict checks are unchanged. This is not generic acceptance of malformed resources, an R5/R6 adapter, or a new package provenance/checksum store.

## Verification

Two extraction tests cover all six definitions, include/exclude references, preparation of normalized systems, unchanged ordinary resources and preservation of unrelated/conflicting input. They construct local synthetic archives and require no network. HTTP client proxy discovery is disabled only in these extraction test fixtures.

The full workspace passes 29 Rust tests, including four database scenarios, and Clippy with warnings denied. All twelve benchmark harness tests pass. Importing the unchanged official R4 core archive now commits all 2,458 terminology resources: 1,062 CodeSystems, 1,316 ValueSets and 80 ConceptMaps. The twelve legacy definitions/references are explicitly reported as normalized.

The tested source archive SHA-256 is `ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44`. Downloaded data and import/test logs remain under ignored `.benchmarks/`; the archive is not required by the automated extraction tests and is not committed. A successful import and these selected checks do not establish complete R4 core terminology support.
