# Native SNOMED RF2 Snapshots

November 3, 2026. The CLI now loads locally supplied Concept, Description, inferred Relationship and Language Snapshots into native tables. Lookup, CodeSystem validation and bounded subsumption use those projections. This is not full SNOMED/ECL support or a clinical benchmark result.

## Supported input

Copy `benchmarks/snomed-rf2.template.json` beside the extracted release, fill each source SHA-256 and verify the complete edition before setting `completeSnapshot=true`. Paths are relative to the manifest. The template intentionally cannot import unchanged.

- One complete **merged Snapshot per component**; filenames must identify Snapshot and end in the declared `_YYYYMMDD.txt` release. Full, Delta, standalone incomplete extensions and unpublished rows are not supported. The loader does not merge International and national extension archives for you.
- Explicit edition module, release date, display dialect and language. The edition module must occur in Concept rows; the dialect must have active members. File hashes and dates are checked, but authenticity/completeness still require the operator's source verification and attestation. Numeric SCTID shape is checked, not Verhoeff check digits or every SNOMED authoring rule.
- All concept lifecycle/module/definition-status fields, descriptions, language memberships and relationship rows/groups are retained. Active relationships must be inferred and use the existential modifier. Inactive rows are retained without using them as active taxonomy edges.
- Display is the unique active preferred synonym for the selected language/dialect, falling back to FSN, never an arbitrary first synonym. Active concepts require active FSNs; inactive concepts may retain the latest inactive FSN. Ambiguous preferred synonyms or active FSNs are rejected. Other terms/dialects remain stored for later richer read support.
- No archive download/extraction, OWL classification, concrete relationships, historical associations/maps, other reference-set types or runtime display-language switching is implemented here. Concept existence over the attested Snapshot is not proof of those semantics.

Sources: SNOMED International [Concept](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/component-release-file-specification/4.2-file-format-specifications/4.2.1-concept-file-specification), [Description](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/component-release-file-specification/4.2-file-format-specifications/4.2.2-description-file-specification), [Relationship](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/component-release-file-specification/4.2-file-format-specifications/4.2.3-relationship-file-specification), [Language Reference Set](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/reference-set-release-file-specification/5.2-reference-set-types/5.2.2.1-language-reference-set) and [version URI](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard/3-snomed-ct-uris-in-use) specifications. Clinical files stay outside Git and must be obtained under their applicable content terms.

## Storage, activation and limits

Migration **`20261103000001_snomed_rf2`** adds `code_systems.native_provider` plus release, description, relationship and language tables. Apply it before the new server starts; startup still does not migrate automatically. Native foreign keys cascade by CodeSystem/release and language-description identity. Logical references to concept projections are validated before activation, without adding a foreign key that would break the existing projection `TRUNCATE` contract.

Each source is decoded into bounded COPY chunks and staged in temporary PostgreSQL tables in one transaction. CSV escaping preserves literal quotes, backslashes and Unicode. References, FSNs/preferences and the inferred DAG are validated before publishing. Kahn elimination uses linear-size temporary graph state, not a release-sized Rust map or materialized transitive closure. The loader never constructs a whole-release FHIR JSON resource and never sends one SQL statement per concept.

Metadata, projected concepts and all native tables become visible together at commit. Source/context fingerprints exclude local paths; identical imports are idempotent after source verification, while an existing conflicting release is refused. Dry-run and failures/cancellation publish nothing and do not advance the persistent generation. Native table statement triggers participate in the existing transactional freshness mechanism. Direct SQL mutations must preserve source/projection invariants; triggers do not automatically recompile arbitrary description edits into display projections.

Limits: 64-KiB manifest, 64-KiB input line, 2-GiB source file, 2,000,000 concepts and 10,000,000 rows per other component; two queued COPY chunks; inferred hierarchy depth at most 64. Each SQL statement has a 15-minute timeout and lock acquisition a five-second timeout. PostgreSQL staging/index/WAL/disk space and overall RSS are separate from these parser/admission bounds. Full-release import cost is not measured yet.

The public CodeSystem resource uses `content=not-present`: it does not pretend to contain all native concepts as JSON. The native provider separately knows that the attested code projection is complete. The business version is `http://snomed.info/sct/{editionModule}/version/{release}`; default selection remains the latest registered resource, while explicit versions and instances preserve their context.

Concept reads reuse the bounded projection cache and SQL fallback. Native hierarchy cache entries are SQL markers, not empty finite closures. One atomic statement resolves metadata/codes/active inferred is-a ancestry, with a total bound of 8,192 `(direction, code)` entries. Exceeding it returns an explicit error even if a positive relationship was found earlier; partial answers are never scored. Generic finite closure behavior is retained. Unsupported post-coordination/expression inputs return an explicit error, not an invented invalid-code result; ordinary non-native codes containing punctuation remain unchanged.

**Native ValueSet compose/implicit expansion, membership filters and ECL are not implemented.** The finite JSON adapter explicitly rejects a native dependency rather than returning an empty successful expansion. Do not remove that guard before adding a bounded native provider. Readiness to import source files does not complete the twelve SNOMED benchmark scenarios.

## Run

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo build --release --bin backend --bin term-squid-cli
.benchmarks/target/release/term-squid-cli --database-url "$DATABASE_URL" migrate
# Restart the updated server before importing native content.
.benchmarks/target/release/term-squid-cli --database-url "$DATABASE_URL" \
  import-snomed --manifest .benchmarks/clinical/snomed/manifest.json --dry-run
.benchmarks/target/release/term-squid-cli --database-url "$DATABASE_URL" \
  import-snomed --manifest .benchmarks/clinical/snomed/manifest.json
```

`just bench-migrate` now uses this first-party CLI command, so setup does not depend on an ignored local helper or installed SQLx CLI. A migration downgrade refuses while native releases exist: back up and explicitly remove their CodeSystems first. It must not leave native concepts interpreted by an older empty-closure backend.

## Verification

The workspace passes **64 Rust tests**, including nineteen database scenarios, with Clippy warnings denied. New coverage includes native knowledge/version/instance handling, all four subsumption outcomes, active/inactive/attribute edges, mutation freshness and traversal admission. Import tests cover preferred-term selection/escaping, provenance, immutable versions/idempotence, sixteen malformed-source/graph/context cases, depth/cycle rejection, publication failure rollback, cancellation/retry and guarded downgrade.

The release CLI/server smoke uses original generated five-concept RF2 files in a disposable schema, not clinical data or benchmark-pool samples. It exercises migration up/down/up with SQLx bookkeeping, dry-run, import/reimport, a bad checksum and thirteen actual HTTP lookup/validation/subsumption/unsupported-compose checks. Source and binary fingerprints and responses are retained under `.benchmarks/profiles/snomed-rf2/`; they do not travel with Git.

The existing benchmark database must remain unchanged apart from the additive migration. Its complete clinical corpus has not been supplied. The original twenty-test clinical suite and composite remain unmeasured on complete data; no speedup, full-release performance or superiority over Termbox/HTS is claimed from these fixtures.

Verified release binaries: backend SHA-256 `ea3af1ba49d9f76dd40712f91e74b288ae5dd501ea0069d533f465b1e5aa6d43`; CLI SHA-256 `c204cc08c68b85357e18bc867baa0a527afeb084d5db856dc0bb38dfc8f329ee`. The actual local migration preserved 1,065 CodeSystems, 1,317 ValueSets, 81 ConceptMaps and 40,861 concept projections; zero native clinical releases were added to that benchmark corpus. Its unchanged local gates pass 7/7 and upstream coverage remains 2/20.

## Subsequent native ValueSet increment

The original importer/read results above are preserved. The [native ValueSet report](SNOMED_VALUESET_REPORT.md) describes the later implicit/compose provider, bounded pages, fresh snapshots and generated performance diagnostics. ECL/refsets and mixed providers remain incomplete; the clinical composite remains unmeasured.
