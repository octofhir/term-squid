# RxNorm semantic types and normalized NDC

November 6, 2026. This extends the [current-active RxNorm provider](RXNORM_READINESS_REPORT.md) with optional monthly semantic-type and attribute sources. Existing uncommitted work is preserved. No clinical archive, deployment or new comparative performance result is supplied.

The subsequent [ECL/RXNREL increment](ECL_RXNORM_RELATIONS_REPORT.md) adds optional direct relationship filters and migration `20261106000002_rxnorm_relationships`. Its sixteen-trigger readiness contract and 322-test checkpoint supersede the relationship/readiness limits and counts below; this report retains the earlier STY/SAT evidence.

## Import and immutable source cohorts

Apply **`20261106000001_rxnorm_auxiliary`**, as well as all earlier migrations, before starting the updated backend. Startup does not migrate. Native import remains CLI-only:

```sh
term-squid-cli --database-url "$DATABASE_URL" migrate
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json --dry-run
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json
```

Start from the [auxiliary template](../benchmarks/rxnorm-auxiliary.template.json), which intentionally fails unchanged. The existing CONSO-only template still works. Alongside the required `source`, either or both optional descriptors may be declared:

```json
{
  "semanticTypes": {
    "path": "RXNSTY.RRF",
    "sha256": "REPLACE_WITH_LOWERCASE_SHA256",
    "expectedRows": 123,
    "version": "04062026",
    "fullMonthly": true
  },
  "attributes": {
    "path": "RXNSAT.RRF",
    "sha256": "REPLACE_WITH_LOWERCASE_SHA256",
    "expectedRows": 456,
    "version": "04062026",
    "fullMonthly": true
  }
}
```

These are illustrative descriptor fragments, not valid complete manifests or real checksums/counts. Both must attest the same full monthly version as CONSO. STY requires at least one row; SAT may be explicitly loaded with zero rows. Omitted/null descriptors mean unavailable source, not a known empty source. Checksums, counts and attestation do not prove clinical authenticity or completeness.

The parser follows the exact six-column RXNSTY and thirteen-column RXNSAT formats in the [NLM technical documentation](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html). All raw fields and rows are retained, including repeated SAT rows, empty ATUI, external/suppressed attributes and unsupported attachment kinds. Row ordinals preserve source identity; no attribute value is used as a unique key.

Validation checks references, exact TUI/STN identifier grammar, consistent TUI labels/tree numbers, duplicate semantic assignments and STY coverage of every CONSO RXCUI, including nonprojected codes. AUI attributes must reference an atom with the same RXCUI; CUI attributes must reference a source concept. Unsupported relation/other attachments remain raw rather than acquiring invented semantics.

All declared files share staging, validation, advisory locking and one publication transaction. Bounds remain 64 KiB manifest, 4 GiB per source, twenty million rows per source, one MiB record and two queued COPY chunks. A combined conservative four MiB per-concept property charge is checked before auxiliary JSON aggregation; no facts are truncated. Dry run, failure and cancellation publish nothing.

CONSO-only metadata/fingerprints remain byte-identical, including absent/null optional descriptors. Auxiliary cohorts add revision `semantic-ndc-v1` and source hashes/counts/version/context to identity; local paths remain excluded. All files must be chosen at first publication: adding, removing or changing a source for an existing edition is an immutable conflict, not enrichment. Use an explicit future rebuild/migration workflow if an already published edition needs different content. Identical cohorts remain unchanged without refreshing activation time.

## Lookup and ValueSet behavior

Loaded STY exposes `TUI` as `valueCode`, and `STN`/`STY` as `valueString`. Loaded SAT exposes only supported curated `NDC` values as strings. Property definitions use `urn:term-squid:rxnorm:{code}`; NDC is a custom property, not a new standard R4 operation.

Explicit lookup requests for `TUI`, `STN`, `STY` or `NDC` require the selected edition's corresponding source. Missing source returns **422**, including older editions of a now enriched canonical. Default/`*` lookup remains usable and returns available properties. A loaded source without a supported value for a known active concept returns no such property, not an unavailable-source error. Availability is never inferred from matching rows.

Supported filters:

| Property | Operators | Values |
|---|---|---|
| `STY` | `=`, `in` | Default TUI; explicit `TUI:`, `STN:` or `STY:` qualifier |
| `NDC` | `=`, `in` | Exact normalized eleven ASCII digits, preserving leading zeros |

The STY default and qualifiers follow [FHIR R4 RxNorm](https://hl7.org/fhir/R4/rxnorm.html). Examples: `T121`, `STN:A1.4.1`, `STY:Pharmacologic Substance`, or `in "TUI:T109,T121"`. The first qualifier applies to subsequent unqualified entries; repeated identical qualifiers work, mixed qualifiers fail. Lists contain at most 256 bounded values. Exact-name `=` allows a comma within a name; `in` uses comma as its delimiter. Unknown qualifiers, unsupported operators, malformed identifiers and invalid NDC formatting fail explicitly rather than becoming empty successes.

Auxiliary filters do **not** establish complete/active scope. Add `rxnorm-status=active`, `activeOnly=true` or another existing supported scope when enumerating the partial provider. For example:

```json
{
  "system": "http://www.nlm.nih.gov/research/umls/rxnorm",
  "filter": [
    {"property": "rxnorm-status", "op": "=", "value": "active"},
    {"property": "STY", "op": "in", "value": "TUI:T109,T121"}
  ]
}
```

Source availability is checked before evaluating another empty predicate, count-zero page or early positive membership. Release-scoped existential predicates preserve unique concepts and exact totals/pages even when raw SAT contains duplicates. Include/exclude, imports, mixed finite/native composition, canonical/version/instance selection and one-operation repeatable-read snapshots remain intact. Capabilities advertise auxiliary operations only for editions with the corresponding source flag. Raw changes participate in generation/freshness guards; no TTL or stale-result fallback is added.

## Conservative NDC projection

Projection requires SAT `SAB=RXNORM`, `ATN=NDC`, `SUPPRESS=N`, an AUI attached to the same RXCUI's active RXNORM product atom, and a current active primary product TTY of `SCD`, `SBD`, `GPCK` or `BPCK`. Values must already have eleven digits. Relevant curated candidates with unsupported attachment/owner/type/format fail the import; they are not silently omitted. A curated NDC attached to multiple distinct active RXCUIs also fails.

This is the current NLM-curated normalized subset described by [NLM's technical documentation](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html) and [getNDCs](https://lhncbc.nlm.nih.gov/RxNav/APIs/api-RxNorm.getNDCs.html), not every source attribute named NDC. External/suppressed/noncurrent rows remain raw, without padding, hyphen removal, source-code guessing, brand propagation, inferred maps or historical equivalence. Leading zeros survive lookup and matching.

Metadata remains `content=not-present`. Unknown/obsolete/archived RXCUIs still return unavailable knowledge, not definitive invalid-code or false membership results. A known active code outside a supported set can produce a valid negative result. Relationships, remapping/history, UNII and arbitrary SAT semantics remain unsupported. The six public R4 operations and seventy declared inputs are unchanged.

## Schema and readiness

The migration adds raw semantic-type/attribute tables, generation triggers and explicit loaded flags plus validated per-file provenance. Legacy editions remain auxiliary-unavailable without changed fingerprints. An empty but declared SAT cohort is distinct from an absent file.

`/ready` now checks the auxiliary tables, **fifteen** enabled tracking triggers and bounded registration/source-descriptor consistency, alongside the prior migration/checksum/version checks. It retains two-second overall, one-second SQL and four-probe bounds. It does not scan clinical corpora or establish complete semantics.

Downgrade locks publication targets before checking absence and refuses to discard declared or raw auxiliary content. A CONSO-only database can downgrade without changing IDs, fingerprints or provenance. Tests cover concurrent real publication versus downgrade, not only static guards.

## Verification and evidence

The final workspace passes **281 tests, zero ignored**: 167 backend, 33 CLI, 70 core and 11 ingestion. Twenty new test functions cover all four source combinations, loaded-empty SAT, raw preservation, 25 atomic invalid-source variants, immutable identity, dry run/rollback/cancellation, projection budgets, exact filters/properties, editions/freshness, mixed composition, readiness and safe downgrade. Existing assertions remain intact. Independent review found and fixed the source TUI/STN grammar gap; both importer and filter now use the same core validator.

Also passing: workspace Clippy with `-D warnings`, formatting, standalone core consumer plus dependency-boundary guard, **38 harness tests / 40,251 assertions**, and four ecosystem-adapter tests. The consumer now exercises public typed STY/NDC parsing without SQL/async/HTTP dependencies.

An actual debug CLI-to-HTTP run passes **26 HTTP checks** in its own temporary local database. It migrates, imports CONSO-only, rejects same-edition auxiliary enrichment, dry-runs and atomically publishes a new complete generated cohort, then verifies idempotence, typed lookup, STY qualifiers/lists, exact NDC/leading zeros, source/suppression exclusion, totals/count zero, old-edition errors, active scope and historical uncertainty. Four STY and five SAT raw rows, including repeated NDC and external/suppressed attributes, are retained. The runner stops its backend and drops only its own database.

Raw logs, source hashes, generated fixtures and the smoke runner/results remain under `.benchmarks/rxnorm-auxiliary-20261106/`; readiness-worker logs also remain in `.benchmarks/mixed-valueset-20261106/readiness-auxiliary-*.log`. Initial attempts are retained separately from final verification. See the [plan tracker](../vibe/rxnorm-auxiliary-plan-track.md).

No production/frontend/browser rebuild, full official ecosystem audit, clinical import, matched FHIRsmith comparison, composite rerun, commit, push or deployment is claimed. This increment extends real provider behavior; it does not establish the clinical product/performance goal.
