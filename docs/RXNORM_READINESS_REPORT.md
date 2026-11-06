# RxNorm, readiness and UCUM tools

November 6, 2026. Three feature groups follow [mixed-provider ValueSets](MIXED_VALUESET_REPORT.md). Prior uncommitted work remains. No actual clinical release, new comparative performance result or deployment is included.

The subsequent [auxiliary-source increment](RXNORM_AUXILIARY_REPORT.md) adds optional RXNSTY/RXNSAT cohorts, STY filters and curated normalized NDC. Its migration `20261106000001_rxnorm_auxiliary`, fifteen-trigger readiness contract and 281-test checkpoint supersede the source/readiness limits and counts below; this report preserves the earlier CONSO-only delivery evidence.

The later [ECL/RXNREL increment](ECL_RXNORM_RELATIONS_REPORT.md) adds optional relationship sources/direct filters, migration `20261106000002_rxnorm_relationships`, sixteen-trigger readiness and a 322-test checkpoint. Earlier unsupported-source statements below describe their original delivery only.

## Native current-active RxNorm

### Import

`import-rxnorm --manifest FILE [--dry-run]` accepts a locally supplied full monthly `RXNCONSO.RRF`. The manifest requires format `rxnorm-rrf-v1`, an explicit `fullMonthly` attestation, **MMDDYYYY** business version, lowercase raw-byte SHA-256, exact raw row count and exact projected active concept count. The [template](../benchmarks/rxnorm-rrf.template.json) intentionally fails unchanged. Checksums/counts and operator attestation do not prove authenticity or clinical completeness. Obtain the actual source under its content terms; weekly/Prescribable subsets are not substitutes for a full monthly file.

The parser accepts UTF-8, an initial BOM, LF/CRLF and a final record without newline. It requires exactly eighteen fields and a trailing pipe, without CSV quote interpretation. All source atoms and all eighteen source fields are retained. RXCUI is the concept identity; RXAUI is the unique atom identity; source `CODE` never replaces RXCUI. Positive scalar source identifiers are bounded to eighteen digits.

Limits are 64 KiB manifest, 4 GiB source, twenty million rows, one MiB record and two queued COPY chunks. A conservative four MiB per-concept property/designation charge gates JSON aggregation. Source data is never retained as a release-sized Rust collection. Malformed input, unsupported active primary TTY, ambiguous/missing/foreign-language primary names, duplicate atom identity, checksum/count mismatch and oversized projection fail before publication. Atoms/designations are never silently dropped or truncated.

Staging, validation, schema/version advisory locking and metadata/atoms/concepts/provenance activate atomically. The fingerprint includes projection revision `current-active-v1`, source hash, counts, version and full-monthly context, not the local path. Identical imports are unchanged without refreshing activation time; changed same-version content fails. Dry runs, cancellation, concurrent identical activation and publication errors preserve transaction/generation atomicity.

### Provider and completeness

The canonical is `http://www.nlm.nih.gov/research/umls/rxnorm`. Projection requires an active `SAB=RXNORM`, `SUPPRESS=N` normalized primary name with an unambiguous recognized primary TTY; rare ET is included. PSN/SY/TMSY remain aliases. The unused `ISPREF` field does not select display. See [FHIR R4 RxNorm](https://hl7.org/fhir/R4/rxnorm.html), [NLM term types](https://www.nlm.nih.gov/research/umls/rxnorm/docs/appendix5.html) and [RRF format](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html).

Supported features:

- Known-active RXCUI lookup/validation with canonical/version, instance and date selection, actual normalized display and typed properties.
- Real active RXNORM alias designations and case-insensitive RxNorm display validation, preserving source spelling. English aliases do not arbitrarily replace primary display. Explicit display-use supplements and other-language supplements retain the existing selection rules.
- Literal code/primary/eligible-alias expansion search, without duplicate concepts or replacing display.
- Explicit/scoped ValueSets, imported intersections/exclusions, deterministic totals/pages and mixed native/finite composition.
- Atom-backed `TTY`/`SAB` filters with `=`/`in`: TTY uses RXNORM atoms except SY, not just primary projection properties; SAB requires an actual source atom for that RXCUI.
- Official all-CUI ValueSet identifier `http://www.nlm.nih.gov/research/umls/rxnorm/vs`, with known-active membership and explicitly constrained enumeration.

Broad expansion requires explicit known-active concepts, `rxnorm-status=active`, `source-table=RXNCONSO.RRF`, `inactive=false`, `activeOnly=true` or `compose.inactive=false`. Active scope propagates through imports and is checked independently for shared scoped/unscoped paths. TTY/SAB alone do not establish historical completeness.

Metadata remains **`content=not-present`**. Absent, obsolete, remapped, quantified or otherwise nonprojected RXCUIs return an explicit unavailable-knowledge error, not a fabricated definitive invalid-code answer. SUPPRESS O/Y/E remain distinct raw states, not an invented inactive classification; see [NLM statuses](https://lhncbc.nlm.nih.gov/RxNav/APIs/api-RxNorm.getRxcuiHistoryStatus.html). A known active code outside an explicit set can still produce a valid negative membership result.

Subsumption, STY, RXNREL relationships/maps, RXNSAT/NDC/UNII and archived/remapped code knowledge remain unsupported. Temporary resources cannot replace the authoritative native canonical. Non-native stored editions still use the finite engine. Existing snapshot/freshness, supplements, label limits and CPU/native admission remain.

### Migration and commands

Apply **`20261106000000_rxnorm_rrf`** before the new backend. It adds source/release tables, generation triggers and the provider constraint; downgrade refuses while native RxNorm CodeSystems exist. Startup does not migrate. Production builds still require frontend generation before RustEmbed.

```sh
# Copy/fill benchmarks/rxnorm-rrf.template.json beside the actual extracted source.
term-squid-cli --database-url "$DATABASE_URL" migrate
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json --dry-run
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json
```

An active-scoped include uses the RxNorm system and `filter: [{"property":"rxnorm-status","op":"=","value":"active"}]`; add TTY/SAB filters as needed. Native RRF upload is CLI-only. Administrative JSON/FHIR-package uploads and the seventy-input/six-operation R4 contract are unchanged.

## Operational readiness

`GET /health` remains database-free liveness. **`GET /ready`** returns 200 `status=ready` or 503 `status=not-ready`, with `Cache-Control: no-store` and only service/version/status fields.

Each probe uses a fresh read-only repeatable-read snapshot. Limits are two seconds overall including checkout/commit, one-second SQL statements and four probe slots; excess probes return 503. It verifies:

- Required public storage tables belong to the actual current schema.
- All compiled up migrations are successful with exact checksums. Missing/dirty/foreign/newer/mismatched ledgers fail closed.
- Generation/map/hierarchy guards and thirteen actual enabled generation-tracking triggers are valid.
- At most 10,000 native registrations have matching canonical/business-version release metadata. SNOMED/RxNorm release dates match encoded versions; null/missing/mismatched metadata fails closed.

An empty migrated database can be ready. Administration/session tables and clinical/conformance/composite coverage are not readiness prerequisites. The probe does not scan complete concept/atom corpora or certify their contents. Unknown newer migrations deliberately make an older build unready; deploy matching schema/binary versions together.

There is no cached healthy result or stale-error fallback. Pool/statement failure, drift or overload produces a generic body without SQL/database details; the next probe observes recovery. Cancellation frees probe admission, while SQLx may drain database work until its statement timeout. No new configuration is needed.

## UCUM library and CLI tools

`UcumProvider::convert(value, from, to) -> Result<f64>` uses the real bundled grammar for compound, affine temperature and supported nonlinear conversions. It rejects nonfinite input/output, incompatible/invalid units and oversized expressions.

`UcumProvider::canonical(code) -> Result<UcumCanonical>` returns canonical unit and decimal **string** factor/offset, preserving the library's precision. Nonlinear units without an affine representation fail explicitly, even when scalar conversion is supported.

Database-free commands produce JSON with `ucumVersion=2.2`:

```sh
term-squid-cli ucum-convert --value -40 --from Cel --to '[degF]'
term-squid-cli ucum-convert --value 1 --from mg/dL --to mg/L
term-squid-cli ucum-canonical --code km
```

Conversion is finite floating-point scalar arithmetic, not an arbitrary-precision quantity API. Canonical factors/offsets are strings. No FHIR conversion route is invented; the reusable library stays free of HTTP, SQL and Tokio dependencies.

## Validation

The reviewed full workspace passes **261 Rust/PostgreSQL tests, zero ignored**: 155 backend, 26 CLI, 69 core and 11 ingestion. Existing 229 checks remain. New coverage comprises seven RxNorm API scenarios, nine importer scenarios, three core RxNorm, three core UCUM-tool, two CLI-tool and eight readiness scenarios.

Tests cover original field/atom preservation, strict malformed-input/primary-name admission, checksum/counts, dry run/idempotence/immutable conflicts, concurrent activation, cancellation behind a real publication lock and rollback; partial knowledge, filters/aliases, dates/versions, supplements/CodeableConcept alternatives and label/projection bounds; readiness drift/rollback/recovery, pool/probe/statement deadlines; conversion errors and canonical representation.

Cross-review found and corrected unknown active primary TTY admission and readiness RxNorm date/version consistency; regressions cover both. An initial API fixture omitted its expected MTHSPL atom: adding the actual source row preserved the assertion instead of weakening filter semantics. Intermediate compile/sandbox/fixture failures are retained, not counted as successful evidence.

Workspace Clippy across all targets/features with warnings denied, formatting, the extended locked independent core consumer/dependency guard, **38 harness tests / 40,251 assertions** and four ecosystem-adapter tests pass. No dependency/lockfile or original benchmark assertion/scorer/pool change is made.

Final **debug** binaries pass real CLI-to-HTTP smoke in a new disposable database: migration, empty readiness, dry run/no publication, import/idempotent repeat, eleven HTTP checks, scoped/unsupported history behavior, aliases/display/TTY filters and post-import readiness. UCUM commands work with an invalid database environment; incompatible conversion fails. The runner stops its backend and drops only its own database. All RRF rows are generated, not an actual monthly clinical release.

Raw evidence: `.benchmarks/rxnorm-ready-20261106/` (`workspace-reviewed.log`, `clippy-reviewed.log`, `core-consumer.log`, `core-dependencies.txt`, `harness.log`, `adapter.log`, `smoke-final/`). Targeted readiness evidence also remains under `.benchmarks/mixed-valueset-20261106/readiness-eight.log`; importer fixtures are under `.benchmarks/rxnorm-tests/`. Toolchain: Rust/Cargo 1.99.1, Bun 1.4.4 and loopback PostgreSQL 17.10.

No new embedded-production/browser build, general ecosystem audit, clinical preflight/composite or comparative performance cohort is claimed. Preserve historical evidence and regressions. Further source work includes RxNorm history/attributes/semantic types/relationships, LOINC accessories and remaining SNOMED semantics; shared versioned Unicode code identity also remains open. No commit, push, publication or deployment is performed.
