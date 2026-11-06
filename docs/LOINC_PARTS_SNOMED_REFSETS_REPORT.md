# LOINC coded primary axes and SNOMED concept refsets

November 6, 2026. Two source-backed feature groups extend the [322-test ECL/RXNREL checkpoint](ECL_RXNORM_RELATIONS_REPORT.md). Earlier working-tree changes remain intact. No clinical archive, deployment or new comparative performance result is supplied.

## Source cohorts and migration

Apply **`20261106000003_loinc_primary_links`** and **`20261106000004_snomed_simple_refsets`**, plus all earlier migrations, before the updated CLI/backend. Startup does not migrate. Both native workflows remain CLI-only:

```sh
term-squid-cli --database-url "$DATABASE_URL" migrate
term-squid-cli --database-url "$DATABASE_URL" import-loinc --manifest .benchmarks/clinical/loinc/manifest.json --dry-run
term-squid-cli --database-url "$DATABASE_URL" import-snomed --manifest .benchmarks/clinical/snomed/manifest.json --dry-run
```

Remove `--dry-run` to publish. The [LOINC template](../benchmarks/loinc-primary-links.template.json) and [SNOMED template](../benchmarks/snomed-simple-refsets.template.json) intentionally fail unchanged. Obtain actual files under their content terms; checksums/counts/operator attestation do not prove authenticity or clinical completeness.

Both sources are optional at **first publication**, not later enrichment. Omitted/null descriptors preserve old metadata, provenance and fingerprints byte-for-byte. Declared source hashes/counts/context and explicit projection revisions join the immutable identity; local paths do not. Identical imports remain unchanged without refreshing activation time. Adding/removing/changing optional sources or declared refset lists for an existing edition conflicts.

All declared files share staging, validation, advisory publication locks and the original transaction. Dry run, malformed data, failure, cancellation and concurrent identical activation never expose partial source/projection/registry changes. No new dependency or lockfile change is added.

## LOINC primary Part links

### Import and coded projection

An illustrative fragment alongside the existing main-table descriptor:

```json
{
  "primaryLinks": {
    "path": "LoincPartLink_Primary.csv",
    "sha256": "REPLACE_WITH_LOWERCASE_SHA256",
    "expectedRows": 123,
    "version": "2.82",
    "completeFile": true
  }
}
```

This is not a complete valid manifest or real checksum/count. Version must equal the main-table version, and the file must be the complete primary-link artifact. Eight exact columns and raw rows are retained, including nonprojected Radiology/DocumentOntology and foreign-system data. Only `LinkTypeName=Primary`, `PartCodeSystem=http://loinc.org` and the six recognized property/type pairs produce coded axes.

Every main-table term, including deprecated terms, must have five required primary axes and Method exactly when its raw method field is nonempty. Eligible axis duplicates, missing/foreign main-term references, mismatched names/type/property, malformed canonical LP identifiers and conflicting Part names fail atomically. This follows the [official primary-link specification](https://loinc.org/kb/enriched-linkages-between-loinc-terms-and-loinc-parts): the full file also contains nonprimary link types; Primary names match main-table fields, and Method is optional. No raw label is guessed into a Part identifier.

| Public coded property | Source `PartTypeName` | Lookup type |
|---|---|---|
| `COMPONENT` | `COMPONENT` | `valueCode` |
| `PROPERTY` | `PROPERTY` | `valueCode` |
| `TIME_ASPCT` | `TIME` | `valueCode` |
| `SYSTEM` | `SYSTEM` | `valueCode` |
| `SCALE_TYP` | `SCALE` | `valueCode` |
| `METHOD_TYP` | `METHOD` | `valueCode` |

Definitions use `http://loinc.org/property/{code}`. Existing `raw-*` string properties remain verbatim and separate. Coded axes are represented as Parts, as described by [FHIR R4 LOINC](https://hl7.org/fhir/R4/loinc.html); they do not become designations or standalone concept registrations.

Bounds are 4 GiB/source, twenty million rows, one MiB decoded/newly read record plus at most eight KiB prefetch, and two queued COPY chunks. Strict quoting, UTF-8, BOM and CRLF checks remain. A conservative combined four MiB per-concept property charge is checked before JSON aggregation. Chunk flush thresholds do not bound total process/PostgreSQL memory. Legacy main-table-only parsing/identity is unchanged.

### Lookup and filtering

Explicit lookup of a coded axis requires that selected native edition's loaded file. Missing source returns **422**; default/`*` lookup still returns available knowledge. A known missing Method from a loaded complete file returns no coded property. Raw-label lookup does not require the Part-link file.

Exact coded `=` is supported. Bounded `in` and boolean `exists` are explicit extensions; this is not a claim of complete R4 LOINC filter/operator coverage. Lists contain at most 256 LP identifiers. Only the ASCII `LP` prefix is case-normalized (`lp100-0` becomes `LP100-0`); digits/check digits are neither padded nor rewritten. A syntactically valid unknown LP literal yields no matching primary links, **not** a conclusion about whether that Part exists in the full terminology.

For example, combine `source-table = Loinc.csv` with `COMPONENT = LP100-0`, or `METHOD_TYP exists false`. Coded axes alone do not establish complete primary enumeration scope. Availability checks precede every include/exclude/import, empty result, zero-count page or early positive membership. Release-scoped existential predicates preserve unique concepts/totals/pages even if privileged direct SQL introduces duplicate raw rows.

Native metadata advertises axes/operators only for loaded editions. Non-native finite editions retain their actual supplied properties without an invented CSV requirement. Selected versions/instances, inactive status, mixed finite/native sets, imports/exclusions, snapshots and committed freshness remain unchanged.

Parts are not registered as independently complete concepts. LP lookup/validation, Part hierarchy, implicit Part sets, whole all-code LOINC enumeration, answer/list/Group/accessory semantics and unsupported regex/hierarchy filters remain explicit errors. Supplementary link meanings and raw Radiology/DocumentOntology rows are not inferred.

## SNOMED simple concept reference sets

### Declared complete Snapshot source

An illustrative optional descriptor alongside the four existing RF2 files:

```json
{
  "simpleRefsets": {
    "path": "der2_Refset_SimpleSnapshot_INT_20260201.txt",
    "sha256": "REPLACE_WITH_LOWERCASE_SHA256",
    "expectedRows": 123,
    "release": "20260201",
    "completeSnapshot": true,
    "conceptOnly": true,
    "refsetIds": ["700001", "700002"]
  }
}
```

These are generated example IDs, not clinical subsets. The release must match the base Snapshot, and one to sixty-four sorted unique scalar refset IDs must explicitly declare coverage. Header-only files are valid complete **empty declarations**. Every raw member belongs to a declared ID; undeclared rows fail, rather than silently expanding source scope.

The reader requires exact six-column RF2 Simple headers, UUID member identities, valid published dates no later than release, 0/1 active flags and known concept references. Every declared refset is active and a proper inferred descendant of `446609009` (the Simple type); the type root itself is not a concrete declared subset. The full bounded ancestry is checked before accepting an early root match. Description/relationship targets, missing concepts, duplicate Snapshot UUIDs, invalid type paths, checksums/counts and over-budget data fail atomically.

Raw six-field strings and inactive rows are preserved. Different UUIDs may repeat the same concept membership without inflating query results. The source follows the [Simple reference-set format](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/reference-set-release-file-specification/5.2-reference-set-types/5.2.1-content-reference-sets/5.2.1.1-simple-reference-set); `conceptOnly` is checked against the actual Concept Snapshot, not inferred from a numeric identifier.

Limits are 2 GiB/source, twenty million rows, 64 KiB records, two queued COPY chunks and 8,192 ancestry nodes per declaration. Registry rows retain every declared refset even without members. No release-sized Rust collection, source-row truncation or per-concept adjacency JSON is introduced.

### Query semantics and inactive distinctions

Three entry points use the selected release's declared source:

- `filter: {"property":"concept","op":"in","value":"700001"}`.
- `http://snomed.info/sct?fhir_vs=refset/700001`, with existing edition/version selectors.
- ECL scalar `^700001`, including optional term annotations and composition such as `(^700001 OR ^700003) AND <<100001`.

These are the [FHIR R4 SNOMED](https://hl7.org/fhir/R4/snomedct.html) refset forms, not a new operation. Ordinary refset filtering selects **active member rows**, which may refer to inactive concepts; existing `activeOnly`/`compose.inactive=false` controls still apply. ECL additionally retains its active-concept substrate. An active refset member does not reactivate its referenced concept, following [RF2 active-field semantics](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/release-types-packages-and-files/3.1-common-features-of-all-release-files/3.1.4-meaning-of-the-active-field).

Every requested ID must be present in the release source list, registered, and represented by an active refset concept, before any result. Missing/undeclared source returns 422; declared empty sets yield valid empty/negative results for known concepts. Wildcard/all-registry selectors, nested refset selectors, projected fields, nonconcept refsets, association maps, refinements and history remain unsupported. ECL byte/depth/aggregate AST/walk bounds and all previous admission/timeout/page guards remain.

Source guards are independent of Boolean short-circuiting, count zero, wrong-system membership and early positives. `EXISTS` preserves membership identity across duplicate raw facts. Implicit refsets use the general native route; the earlier whole/isa single-statement path is unchanged.

A finite stored SNOMED edition cannot turn `concept in` into a literal code list. The compose loader routes expression/refset semantics through source-aware resolution, which explicitly rejects missing RF2 sources before finite fallback. Ordinary finite `code =` still works. Native and finite metadata reflect that distinction; request-local authoritative CodeSystem overrides remain forbidden.

## Readiness and downgrade

The migrations add loaded flags, strict per-file source descriptors, raw storage, bounded declaration registries and three generation triggers. `/ready` now checks **nineteen** enabled tracking triggers and all new tables. Valid descriptors/flags are checked in the same fresh snapshot; registry metadata must exactly match the declared source list, including empty sets. This metadata scan is bounded by the existing native registration limit and at most sixty-four declarations per release; raw member/Part-link corpora are not scanned.

The existing two-second overall, one-second SQL and four-probe bounds remain. Readiness does not establish clinical authenticity, corpus completeness or semantic certification. No auth/session table dependency is added.

Both downgrades lock publication targets before checking absence and refuse declared or unclaimed raw/registry data. Legacy releases can downgrade without changed IDs/fingerprints/provenance. Real lock-wait publication/downgrade regressions verify that committed cohorts cannot be discarded.

## Verification and remaining work

The final workspace passes **379 tests, zero ignored**: 228 backend, 59 CLI, 81 core and 11 ingestion. Fifty-seven new functions cover coded axes, Method absence, raw retention/coverage, strict parsers/budgets, declared empty sets, concept/member inactivity, bounded type proof, all invocation forms, finite fallback protection, mixed composition, pages, editions, rollback/commit freshness, immutable identity, publication/cancellation and readiness/downgrade races. Earlier assertions remain intact; obsolete unsupported-input expectations change only where new support is explicitly tested.

Workspace check/Clippy with `-D warnings`, formatting, independent core consumer/dependency guard, **38 harness tests / 40,251 assertions**, and four adapter tests pass. The standalone consumer now exercises coded axis normalization and refset AST/source dependencies without SQL, async or HTTP dependencies. Independent cross-review covered both importers, both providers, source guards, finite fallback and readiness.

Actual generated-file CLI/HTTP smoke passes **39 checks** after importing both providers into its own disposable local database. It verifies dry run/idempotence/immutable conflicts, all six coded lookup types, exact/list/existence filters, missing Method/old-source errors, active-member versus active-concept behavior, empty declarations, Boolean/hierarchy composition, implicit URLs, pages, mixed providers and unsupported selectors. Thirteen raw primary-link rows, three declared registries and five member rows (including duplicate/inactive facts) are retained. The runner stops/drops only its own process/database. Raw fixtures, initial/final logs, source/archive/binary hashes and exact results remain under `.benchmarks/loinc-parts-snomed-refsets-20261106/`. The first smoke's omitted required `DefinitionDescription` column was corrected in the generator, not by relaxing the importer; its fixtures/log remain retained. See the [tracker](../vibe/loinc-parts-snomed-refsets-plan-track.md).

No new production/frontend/browser build, official full ecosystem audit, clinical import, matched FHIRsmith comparison, composite score, commit, push or deployment is claimed. Actual complete releases, broader LOINC accessories, advanced ECL/nonconcept refsets, mappings/history, Unicode identity policy and measured full-release cost remain open.
