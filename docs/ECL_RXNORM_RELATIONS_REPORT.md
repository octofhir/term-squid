# Bounded SNOMED ECL and RxNorm relationships

November 6, 2026. Two feature groups extend the [281-test auxiliary checkpoint](RXNORM_AUXILIARY_REPORT.md). Earlier working-tree changes remain intact. This delivery contains no clinical archive, new performance comparison, commit, push or deployment.

The subsequent [LOINC Parts/SNOMED refset increment](LOINC_PARTS_SNOMED_REFSETS_REPORT.md) adds declared concept refsets and scalar ECL `^`, with nineteen-trigger readiness and a 379-test checkpoint. It supersedes the blanket refset limitation below, not the remaining selectors/refinements/history or clinical-performance limits.

## SNOMED expression-constraint queries

Native RF2 ValueSets now support `constraint =` and the implicit canonical form `http://snomed.info/sct?fhir_vs=ecl/[encoded-expression]`, with the existing edition/version selectors. These are the entry points defined by [FHIR R4 SNOMED](https://hl7.org/fhir/R4/snomedct.html), not a seventh operation.

The dependency-free core exposes a typed `snomed::Ecl` AST/parser. Supported brief syntax:

| Form | Meaning |
|---|---|
| `100001`, `100001 \|optional term\|` | One scalar focus concept; term text is annotation, not display validation |
| `*` | Active concepts in the selected edition |
| `<`, `<<` | Descendants, excluding/including self |
| `>`, `>>` | Ancestors, excluding/including self |
| `<!`, `<<!` | Direct children, excluding/including self |
| `>!`, `>>!` | Direct parents, excluding/including self |
| `AND`, comma, `OR` | Intersection/conjunction and union/disjunction |
| `MINUS` | Binary set exclusion |
| Parentheses | Explicit grouping |

Hierarchy operators in this subset take one scalar identifier, not a nested set or wildcard. Identifiers have six to eighteen digits with no leading zero; this lexical check is not SCTID authenticity verification. Keyword case is insensitive. As specified by the [ECL grammar](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language/design/5-syntax-specification), mixed binary operators and repeated `MINUS` need parentheses; there is no invented AND-before-OR precedence.

Example include:

```json
{
  "system": "http://snomed.info/sct",
  "filter": [{
    "property": "constraint",
    "op": "=",
    "value": "(<< 100001 MINUS << 100002) OR <! 100003"
  }]
}
```

This is generated example syntax, not a clinical definition. Implicit expressions must be URI-encoded, including spaces and term delimiters. Decoding happens exactly once, requires valid UTF-8/percent escapes and never treats `+` as a space. Encoded query/fragment characters can occur as term annotation, but cannot inject outer query parameters.

### Active substrate and bounded execution

ECL uses active concepts and active inferred `is-a` relationships in the selected RF2 edition. Unknown or inactive focus concepts fail explicitly, even in an otherwise successful union or empty conjunction. Every traversal starts on an active concept and checks active intermediate/destination concepts. Stated, inactive and non-`is-a` edges do not contribute. Direct operators follow one source edge in the supplied inferred distribution-normal-form graph; no new classifier or graph-reduction engine is added.

Ordinary whole-system/`isa` filters retain their prior inactive behavior and single-statement membership fast path. ECL uses the general native provider. Membership traverses upward from the candidate for descendant constraints, rather than enumerating a whole descendant population. Ancestor constraints retain their actual direction. Cycles terminate through deduplicating recursive walks.

Bounds are **4,096 bytes per expression, 32 parenthesis levels, 128 aggregate ECL AST nodes and sixteen aggregate hierarchy leaves per operation**, including ordinary hierarchy filters. Existing 8,192 membership / one-million expansion walk bounds, 32 MiB page charge, admission, CPU permits, deadlines and statement timeouts remain. Every walk guard is evaluated independently of Boolean short-circuiting, wrong-system membership, empty pages or early positives; no partial result becomes success.

SQL identifiers come only from typed operators/generated names; all source identifiers remain bound values. No release-sized Rust set, new cache/TTL, migration, source projection or dependency is needed for ECL. Inline/stored/instance/contained/imported sets, exclusions, finite/native unions, exact totals/pages, language/labels, date/version selection and committed freshness keep one-operation snapshot semantics. TerminologyCapabilities advertises `constraint` and explicitly describes the subset and limits.

Refsets, attribute refinements/cardinality/groups, dotted expressions, description/concept filters, history supplements, alternate identifiers, hierarchy operators on nested sets/wildcards and post-coordinated expression evaluation remain unsupported. They return errors, not empty sets or scalar-code invalidity. This is not full ECL certification.

## Atomic RxNorm relationship sources

Apply **`20261106000002_rxnorm_relationships`** and all earlier migrations before the updated backend/CLI. Startup does not migrate. Use the [relationship template](../benchmarks/rxnorm-relationships.template.json), which intentionally fails unchanged. Add an optional `relationships` descriptor to the existing RxNorm manifest:

```json
{
  "relationships": {
    "path": "RXNREL.RRF",
    "sha256": "REPLACE_WITH_LOWERCASE_SHA256",
    "expectedRows": 123,
    "version": "04062026",
    "fullMonthly": true
  }
}
```

This fragment is not a complete valid manifest. Version/full-monthly attestation must match CONSO and any STY/SAT descriptors. Zero-row declared files establish known absence; omitted/null sources are unavailable. All declared files are checked and published in one transaction through `import-rxnorm --manifest FILE [--dry-run]`.

The importer preserves all sixteen source fields, raw order/duplicates, blank CUIs on atom relationships, opaque UMLS endpoints, empty RUI, source labels and suppression markers. File columns RG/DIR retain their documented positions, rather than a loader-script ordering. Numeric CUI/AUI references are validated against the exact full CONSO cohort, with atom/concept ownership consistency. External opaque endpoints remain raw and never acquire guessed RxNorm identities. See the [NLM RRF specification](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html).

Bounds remain 4 GiB/file, twenty million rows, one MiB/record and two queued COPY chunks; indexed scalar fields are bounded to 255 bytes. Relationship storage is row-based, without unbounded adjacency JSON aggregation. All-source staging, rollback, dry run, publication locking and cancellation remain atomic.

Absent/null REL leaves old CONSO-only and STY/SAT cohort metadata/fingerprints byte-identical. Present REL adds `relationshipProjection=relation-v1` and immutable source hash/count/version/context, not local paths. Adding/removing/changing REL on an already published edition conflicts; all files must be chosen at first publication. Identical cohorts are unchanged without renewed activation time.

### Standard direct REL/RELA filters

Supported operators are `=`/`in` for the seven R4 REL properties (`SY`, `SIB`, `RN`, `PAR`, `CHD`, `RB`, `RO`) and twenty-eight named RELA properties in the [official inventory](https://www.nlm.nih.gov/research/umls/rxnorm/docs/appendix1.html). Values require an explicit first `CUI:` or `AUI:` namespace; subsequent list entries inherit it. Repeated identical qualifiers work; mixed namespaces fail. Lists have at most 256 positive bounded identifiers. Unknown relationship names, malformed values and unsupported operators fail explicitly.

**Direction matters:** the [R4 SQL contract](https://hl7.org/fhir/R4/rxnorm.html) selects endpoint 1, anchored at endpoint 2. For a source row with RXCUI1 ingredient, RXCUI2 drug and RELA `has_ingredient`, filtering `has_ingredient = CUI:drug` returns ingredients, not drugs. AUI filtering resolves candidate RXAUI1 through the selected release's raw CONSO atoms. No inverse, transitive path or RxNav-style ingredient propagation is invented.

Candidates remain known active normalized RXCUIs. Anchors are validated against full raw CONSO, including source-only/noncurrent concepts and atoms as allowed by R4. Relationship rows from foreign SABs can provide real source facts; they are not discarded merely for lacking `SAB=RXNORM`. Current-safe query eligibility requires blank/`N` relationship suppression; `Y`, `E` and `O` remain raw but do not match.

Every selected edition must have its relationship source loaded, and every anchor must exist, before count-zero, empty or early-positive results. Missing source/unknown target returns 422; loaded-empty is known absence for a known active candidate. Duplicate rows use existential predicates and cannot inflate totals. Relationship filters do not establish active enumeration scope. Mixed imports/exclusions, auxiliary predicates, paging, editions and fresh committed observations remain unchanged.

No relationship adjacency lookup properties, RxNorm subsumption, mapping equivalence, transitive closure, historical classification or UNII semantics are added. In particular, direct `isa`/`inverse_isa` source filters do not enable `$subsumes`. Unknown/noncurrent candidate existence remains unavailable rather than definitively false. The six-operation/seventy-input R4 boundary is unchanged.

### Storage and operational readiness

The migration adds ordinal-keyed `rxnorm_relationships`, four partial endpoint/query indexes, a generation trigger and a separately validated loaded flag/source descriptor. It does not change the older auxiliary migration or its validator. `/ready` now checks **sixteen** tracking triggers, required relationship storage and bounded source-descriptor consistency, with the existing two-second overall, one-second SQL and four-probe bounds. It does not scan a clinical corpus.

Downgrade locks publication targets before checking absence and refuses loaded or unclaimed raw relationship data. Legacy editions can downgrade without changed IDs/fingerprints/provenance. Concurrent publication/downgrade tests verify that committed cohorts cannot be discarded.

## Verification

The workspace passes **322 tests, zero ignored**: 194 backend, 41 CLI, 76 core and 11 ingestion. Forty-one new test functions cover parser bounds, all eight hierarchy operators, active intermediates, ambiguity/unsupported syntax, URI handling, cycles, mixed sets, editions, cache parity/freshness, non-partial traversal guards, relationship direction/namespaces/source facts, raw preservation, seventeen invalid import variants, legacy identity, atomic publication and readiness/downgrade races.

Workspace check and Clippy with `-D warnings`, formatting, the independent core consumer/dependency boundary, **38 harness tests / 40,251 assertions**, and four adapter tests pass. The consumer exercises typed ECL/relationship parsing without database, async or HTTP dependencies. Independent cross-review covered both providers and the importer/schema; a stale metadata scope description was corrected and regression-tested. Initial fixture-encoding and outdated negative-assertion failures remain in raw logs alongside final results.

Actual generated-file CLI-to-HTTP smoke passes **56 HTTP checks** after importing both RF2 and a complete declared RxNorm source cohort into its own disposable local database, with dry run/idempotence and immutable-source conflict checks. It verifies real query responses, not only HTTP status, and stops/drops only its own process/database. The first smoke incorrectly expected 422 for RxNorm subsumption; the existing no-`is-a` contract is 400 with OperationOutcome. The corrected runner asserts that exact response without changing server behavior. Exact counts/results, source hashes, fixtures, initial/final logs and runner live under `.benchmarks/ecl-rxnorm-relations-20261106/`. See the [tracker](../vibe/ecl-rxnorm-relations-plan-track.md).

No new production/frontend/browser build, official full ecosystem audit, licensed clinical import, matched FHIRsmith run or composite score is claimed. Real full-release cost, clinical content, unsupported ECL/refsets, LOINC accessory files and historical knowledge remain open product work.
