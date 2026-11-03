# Request-local ValueSets

November 3, 2026. This increment implements inline `valueSet.resource` inputs for R4 `ValueSet/$expand` and `ValueSet/$validate-code`, using the existing finite compose engine. No migration is required. The original clinical benchmark scripts and pools are unchanged.

## Contract

- POST a FHIR `Parameters` resource with one `valueSet` parameter containing a `ValueSet` resource and its `compose`. The inline resource may omit `url`, `id` and `version`.
- This supported subset accepts exactly one root selector: instance, `url`, or inline `valueSet`. Duplicate selectors, instance/inline or url/inline mixtures, wrong resource types and mixed `resource`/`value[x]`/`part` forms return HTTP 400 OperationOutcome. Matching url+inline combinations are not accepted in this subset.
- Optional `valueSetVersion` must match the inline resource's declared version. `systemVersion` on validation selects an unversioned CodeSystem dependency and remains part of the plan key. Explicit dependency versions retain precedence.
- Include/exclude, imported intersections/unions, supported typed/hierarchy filters, case policy, display checks, text filtering and pagination reuse the existing evaluator. Unsupported operators and incomplete terminology remain errors, not empty successful results.
- Unsupported inline operation parameters, duplicate fields and incorrect primitive types are rejected rather than ignored; richer options must be implemented before admission.
- The inline resource is limited to 256 KiB of compact serialized JSON. Existing 32-MiB dependency input, 128-resource, 100,000-concept, depth, page-size and retained-cache limits remain. These are admission limits, not a whole-process RSS bound. Broader Coding/CodeableConcept, language and supplement semantics are still incomplete.

FHIR references: [expand](https://hl7.org/fhir/R4/valueset-operation-expand.html), [validate-code](https://hl7.org/fhir/R4/valueset-operation-validate-code.html), [Parameters](https://hl7.org/fhir/R4/parameters.html).

## Execution and freshness

`Parameter.resource` reaches `compose_inline` on the store. SHA-256 of the whole inline resource distinguishes different definitions and response metadata even when canonical URL/version are identical. Stored roots have a separate key discriminator; an inline request cannot reuse or overwrite the stored root plan. Anonymous roots receive only an internal content-derived key; it is not inserted into their response as a fabricated canonical URL.

Inline roots and their stored dependencies share the existing cold admission lock, read-only repeatable-read transaction and blocking preparation. Each warm operation still obtains a fresh generation observation. Committed dependency changes invalidate plans, rollback leaves the generation unchanged, and generation-query errors do not serve stale results. All plans share the existing eight-entry / 64-MiB ValueSet cache; arbitrary client resources do not get a new unbounded cache. Temporary resources are never written to PostgreSQL.

## Verification

All 51 Rust tests pass, including thirteen PostgreSQL scenarios and four new inline tests. Clippy denies warnings; formatting and 29 benchmark tests pass. New cases cover anonymous/named roots, stored-root isolation, different inline definitions, typed filters, include/exclude/imports, versions, case/display, pages, shared cold preparation, commits/rollback, eviction, missing/incomplete content, cycles, resource limits, exact 256-KiB admission and malformed selectors.

The initial inline-only release binary SHA-256 is `f3de7ef026f06433c15801e2d723f477efb57ab43cff3fda4fec552e2c40a212`. Real local HTTP checks pass six of six: two expansion pages plus included/excluded/outside/missing membership. The unchanged local suite passes 7/7. Raw artifacts remain under `.benchmarks/profiles/inline-valueset/` and `.benchmarks/results/inline-{local,upstream}-audit/`.

The original upstream preflight still has **2/20 passes**, now seven failures / eleven skips. EX02/EX05/EX08 reach the evaluator and report the incomplete SNOMED stub; EX06/EX07 report missing clinical dependencies. The former `url parameter required` failure is gone for all five inline expansion cases. This is an implemented input capability, **not five new clinical passes**. Complete clinical imports, SNOMED filters/implicit sets and the translation compatibility decision remain; no comparable composite or throughput speedup is claimed.

The subsequent combined inline/UCUM release is recorded in [the UCUM report](UCUM_PROVIDER_REPORT.md); its full workspace passes 57 tests and repeats the same inline HTTP checks. Initial artifacts and binaries remain preserved.
