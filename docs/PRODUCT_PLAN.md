# term-squid Product Development Plan

Status: implementation started; milestones define the proposed delivery order. Updated November 4, 2026.

term-squid is a standalone terminology product with its own FHIR API, content, storage, CLI and UI. Its internal engine is designed as a Rust library. Embedding it in OctoFHIR or placing terminology data in the OctoFHIR database is outside the current scope.

The first product release prioritizes local code systems, HL7, LOINC and UCUM. FHIR compatibility is required; performance comes from prepared data, indexes, compact representations and execution design.

## Current execution priority: composite leadership

For cross-device continuation, start with [NEXT_SESSION.md](NEXT_SESSION.md).

The primary performance KPI is now **first place by the unchanged HealthSamurai TX Benchmark composite**, with direct local Termbox and FHIRsmith comparisons. The earlier 3x FHIRsmith and 12k synthetic targets remain secondary diagnostics, not substitutes for that ranking. See [the pinned baseline and current coverage](HEALTHSAMURAI_BASELINE.md).

Current upstream preflight coverage is **2/20** on synthetic + R4 core content; no comparable clinical composite has been measured. The published leaders are Termbox 100.00, Hades 79.20 and FHIRsmith 31.96. These historical scores are not measurements on our machine. The local [dashboard](http://127.0.0.1:4100/tx-benchmark/local/) keeps published scores, coverage and actual synthetic charts separate.

Execute the composite workstream before further synthetic-only tuning:

1. Bounded inline ValueSet inputs for expansion/membership are implemented and tested; see [the inline increment](INLINE_VALUESET_REPORT.md). The five inline expansion scenarios now reach clinical dependency/evaluator checks, but still do not pass on the incomplete corpus. Resolve the upstream translation helper's `target` system-URI usage through an explicit compatibility decision; preserve standard R4 `target` ValueSet semantics.
2. The [native RF2 Snapshot loader and bounded inferred subsumption](SNOMED_RF2_REPORT.md) are implemented, together with the [first native ValueSet/implicit/hierarchy/property subset](SNOMED_VALUESET_REPORT.md). The [implicit membership execution increment](IMPLICIT_MEMBERSHIP_REPORT.md) removes repeated metadata/JSON preparation for simple native sets. Complete mixed providers, native query performance, remaining filters, merged-edition workflows and remaining clinical ingestion for the pinned corpus. SNOMED moves forward from later development because it affects twelve of twenty benchmark tests. Do not implement only sampled pool codes or claim full ECL support.
   The [native LOINC main-table increment](LOINC_PROVIDER_REPORT.md) adds checksummed streaming import, atomic activation, numeric reads, scoped primary filters and mixed native SNOMED/LOINC sets; typed lookup properties are preserved. Load the actual complete 2.82 release for LK02. Parts/answers/hierarchy, broad all-code expansion and native RxNorm remain incomplete.
3. Run the unchanged pools/protocol against separately provisioned local Termbox, FHIRsmith and term-squid with the same content/resource limits. Keep per-test coverage and zero-error timings; compute with the original scorer and weights. Historical different-host results are reference context only.
4. Rank measured per-test composite deficits, profile those exact workloads, then optimize bounded snapshots/indexes/search and large-release fallbacks. Re-run the complete matched cohort after each candidate. A three-server local win is not yet proof of winning against every implementation; include other contenders before that claim.

The [HTS audit](HTS_FEATURE_GAPS.md) identifies relevant input, clinical-import and typed-read gaps. The [UCUM increment](UCUM_PROVIDER_REPORT.md) now reuses pinned `octofhir-ucum` for canonical grammar lookup/validation and reusable Rust comparability; finite ValueSet integration remains separate. HTS has broader functionality today; public authoring/history does not displace the score-critical path. Clinical archives require their own access/content terms and remain outside Git.

All Git-facing code, comments, documentation and reports are written in English. Local working notes, raw profiles, generated datasets and measurement artifacts remain under the ignored `.benchmarks/` directory.

## Contract and References

- Initial supported FHIR API: R4 4.0.1. R5 requires a verified adapter; a route alone does not establish release support.
- Functional reference: [FHIRsmith](https://github.com/HealthIntersections/FHIRsmith), inspected commit `25a41f87711507789b361480ee4efae42df504d4`. Pin the reference version in comparative runs.
- Specification reference: [FHIR R4 Terminology Service](https://hl7.org/fhir/R4/terminology-service.html). A reference server's deviation from the specification requires an explicit decision and test.
- Compatibility checks: [HL7 Terminology Ecosystem tests](https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/testcases.html), with pinned suite versions, runner and modes. Verify behavior with caching enabled and disabled.
- FHIR core, Validator/IG Publisher ecosystem protocols and server-specific extensions have separate capability commitments. Reproducing every additional FHIRsmith service is not required.
- Publish speed together with scenario coverage and measurement conditions. Unsupported operations, incorrect responses and unexpected empty results do not count as fast implementations.

## Milestone 0: Reproducible Baseline

The repository includes a local Bun/k6 runner, synthetic FHIR package, isolated benchmark database and pinned TX Benchmark integration. See [benchmarks/README.md](../benchmarks/README.md).

Available features include semantic preflight, repeated response validation during local load tests, warmup, concurrency levels of 1/10/50, and JSON artifacts with versions and dataset fingerprints. The full upstream clinical suite requires separately loaded terminology content.

The first real run against a separate PostgreSQL 17 instance using the repository migrations confirmed:

| Scenario | Preflight result |
|---|---|
| CodeSystem validation | Passes for existing and missing codes |
| Lookup | Omits the terminology version |
| ValueSet validation | Incorrectly accepts an excluded code and a code outside the set |
| Expansion | HTTP 500: SQL references the nonexistent `expansion` column |
| Subsumption | Returns `not-subsumed` for a parent and child |
| Forward translation | Passes |
| Reverse translation | Does not find an existing mapping |

Local verification artifacts: `.benchmarks/results/audit-preflight/term-squid/`. Short load runs verify the harness; they are not a comparative baseline against FHIRsmith.

The November 2 increment implements the seven local scenarios, extracts `term-squid-core`, and adds separate CPU/allocation profiling and sustained 1/10/50 VU measurements. A local pinned FHIRsmith R4 instance now runs the synthetic fixtures. See [PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md) for results, reference differences and limits; milestones 1 and 2 remain incomplete. The primary five-connection validation result is 4,448 req/s at 10 VU versus 196 req/s before (22.65×); a separate 32-connection experiment peaks at 7,156 req/s. The >12k and overall 3× reference goals are not achieved. Final compose rechecks remain near 52 req/s, making reusable preparation the next measured priority.

The next increment adds bounded immutable finite plans with transactional cross-process invalidation. See [PREPARED_DATA_REPORT.md](PREPARED_DATA_REPORT.md) for evidence and remaining shared-snapshot/admission work. The subsequent [warm-read increment](WARM_READ_REPORT.md) makes generation checks concurrent while retaining post-commit freshness. The [transport increment](TRANSPORT_PERFORMANCE_REPORT.md) evaluates existing OctoFHIR pool and TCP patterns against the shared local FHIRsmith suite. The following [translation increment](TRANSLATION_PERFORMANCE_REPORT.md) adds bounded forward/reverse ConceptMap plans and generation tracking to address repeated resource decoding.

The current [atomic-subsumption increment](SUBSUMPTION_REPORT.md) consolidates system, concept and closure reads into one statement, fixes an exact-case index-range scan and enables database regression tests in CI. All 30 Rust tests, Clippy and seven local semantic scenarios pass; a narrow [R4 core import adapter](R4_CORE_COMPATIBILITY.md) allows the pinned package to load completely. The [matched performance report](SUBSUMPTION_PERFORMANCE_REPORT.md) records 2,922 versus 1,107 previous RPS at 10 VU, but only 0.285x local FHIRsmith throughput. The overall reference objective remains unmet.

The subsequent [six-workload pool selection](POOL_DEFAULT_REPORT.md) raises the server default from five to 32 connections, preserving explicit overrides. The longer confirmation wins against the pinned reference on validation, membership and expansion but still loses on lookup, subsumption and forward translation; no overall 3x or >12k claim is established.

The next [prepared-hierarchy increment](HIERARCHY_REPORT.md) adds immutable closure indexes, PostgreSQL-defined case keys, bounded retention/SQL admission and transactional concept/closure invalidation. All 38 Rust tests, including large folded/fallback and concurrent update coverage, pass. At 50 VU, original subsumption reaches 6,313 RPS versus 9,384 reference; the SQL-baseline gain shrinks to 3.8% in a repeat. A separate folded workload improves 1,285 → 7,289 RPS, but the reference fails semantic preflight and is unscored there. A fresh generation query remains on every warm request; this does not fulfill the database-free hot path or establish the overall performance target.

The subsequent [generation-batching increment](GENERATION_BATCHING_REPORT.md) shares only post-admission SQL observations, with bounded queues, cancellation and HTTP 503 overload. All 40 Rust tests pass. At 50 VU it reaches 19,648 membership / 8,739 expansion / 18,157 subsumption / 18,228 forward-translation RPS, exceeding the pinned reference on all four. Subsumption still loses to the reference at 1/10 VU; the batched build is 2.7% below its baseline at 1 VU. The loaded reverse-order repeat confirms 17,487 versus 9,964 reference RPS. These are the first >12k results on three synthetic prepared reads, not an overall 3x score or clinical/lookup guarantee. Fresh database observations remain mandatory.

The following [prepared CodeSystem-read increment](CONCEPT_READS_REPORT.md) adds bounded immutable lookup/validation payloads with PostgreSQL case keys, one-snapshot metadata, incremental decoded-memory admission and original-SQL fallback. All 47 Rust tests pass. At 50 VU lookup reaches 16,949 RPS versus 5,819 previous / 8,325 reference; CodeSystem validation reaches 18,408 versus 5,631 / 5,105. A lookup repeat reaches 21,265 versus 8,842 reference, so its full 16,949–21,265 range remains visible. Both exceed 12k on the synthetic warm workload, but single-client losses, the 10-VU validation tail regression and added retained memory remain. No overall-suite 3x or clinical/mixed claim is established; fresh database observations remain mandatory.

Remaining work:

1. Connect the ecosystem runner to CI for selected R4/general scenarios and define how its test resources are loaded.
2. Complete the R4 parameter/semantic contract and maintain a requirements matrix beyond the synthetic preflights.
3. Extend bounded finite-plan reuse with shared CodeSystem snapshots, finer admission and dependency-specific generations; add larger and mixed-load comparisons.
4. Check correctness in CI; run absolute performance comparisons on a stable dedicated machine.

Acceptance: a developer can reproduce the dataset, checks and results; known defects remain visible separately from speed measurements.

## Milestone 1: Correct Minimal R4 Server

These tasks block using term-squid as an authoritative validation service.

| Priority | Change | Verifiable behavior |
|---|---|---|
| P0 | ValueSet membership | An included code passes; excluded and outside codes are rejected |
| P0 | Align expansion SQL with the schema | Queries work against a database created by repository migrations |
| P0 | Evaluate compose | Explicit codes, whole systems, include/exclude, imported ValueSets and filters have correct semantics |
| P0 | Correct FHIR reads and errors | Return FHIR resources and OperationOutcome rather than database-row models and custom error wrappers |
| P0 | Version handling | `version`, `systemVersion`, `valueSetVersion`, versioned canonicals and instance operations preserve the selected context |
| P0 | Knowledge completeness | Distinguish an invalid code from an unavailable terminology, version or unsupported filter |
| P0 | GET/POST Parameters | Accept typed primitives, Coding, CodeableConcept and inline resources within the declared contract |
| P1 | Hierarchies and mappings | Recursive import, parent/child relationships, subsumption, forward and reverse translation |
| P1 | Metadata | CapabilityStatement and TerminologyCapabilities describe actual operations, parameters, content and versions |

Display handling, language, inactive/abstract concepts, supplements, case sensitivity and ambiguous system inference are part of the tested semantics. Never silently ignore an unknown rule. Composition requires nesting limits, cycle detection and controlled errors.

Acceptance: local preflights pass; selected ecosystem scenarios produce the same correct results with caching enabled and disabled. Contract completeness is tracked in [the API requirements matrix](API_REQUIREMENTS.md), not just the small local suite.

The [R4 API implementation](API_IMPLEMENTATION_REPORT.md) now covers all **70 top-level inputs of six declared operations**, typed/CodeableConcept/inline workflows, one-snapshot validation, date/language/designation semantics, expansion version/context directives, bounded mapping rules and CodeSystem supplements. The pinned [contract inventory](R4_API_CONTRACT.json) links positive acceptance evidence. Current Rust/PostgreSQL validation passes 147 tests. Dynamic terminology metadata and local OperationDefinition resources describe actual behavior. The final embedded build passes 23 browser/API scenarios, JSON/package restart recovery and all seven unchanged local gates; see the report for raw evidence and toolchain. Provider/registry bounds remain explicit, and clinical corpus/composite goals remain separate; this does not assert unrestricted FHIR support. Apply migration `20261104000006_supplements`.

## Milestone 2: Library Engine and Prepared Data

Start extracting the engine alongside milestone 1 fixes, keeping the server operational at each step.

- `term-squid-core`: typed model, canonical/version resolution, ValueSet plans, code and set operations, provider contracts. HTTP and SQLx are not mandatory engine dependencies.
- Import and storage: the product's own database, content provenance, package dependencies, validation and snapshot construction.
- FHIR adapter: parameter parsing, release adaptation, result representations and OperationOutcome.
- CLI and HTTP server share the engine. The UI uses the product API.

Reads use an immutable snapshot. Build updates separately and activate them atomically; one operation stays on one snapshot. Canonical URLs, terminology business versions and FHIR meta.versionId are distinct identifiers.

For finite sets, compare ID arrays, bitmaps and hash indexes. A prepared plan and its evaluation result are separate cache layers. Keys include the data generation, resolved dependencies and every result-affecting parameter. Compute immutable resource fingerprints at registration, including temporary client resources.

Acceptance: operations execute through the engine; frequent simple validations neither parse resource JSON nor query the database; updates do not mix releases or return stale cached results.

## Milestone 3: Content for the First Release

| Content | Required work |
|---|---|
| Local code systems | Recursive concepts, typed properties, designations, languages, mappings and controlled updates |
| HL7 | Core/THO/IG packages with dependencies, defined source precedence, pinned versions and provenance |
| UCUM | Canonical grammar lookup/validation and Rust comparability now use pinned `octofhir-ucum` 0.9.1 / model 2.2. Complete finite ValueSet/provider integration and ecosystem cases; a common-units list does not replace the grammar |
| LOINC | Import the official release: axes, class, status, names, languages, Parts, hierarchy, Answer Lists and Groups; capabilities declare supported properties and filters |

Imports record version, checksum and source. Load data in streams or batches rather than one SQL round trip per concept. An incomplete import never becomes active. Materialize large expansions as needed within memory limits. Clinical source data stays out of Git; version manifests and permitted test fixtures belong in Git.

Acceptance: reproducible content updates and rollback; correct selected HL7/LOINC/UCUM scenarios against pinned data releases.

## Milestone 4: Demonstrated Speedup over FHIRsmith

Primary target: lead the original HealthSamurai composite with unchanged weights and verified clinical coverage, including direct local Termbox/FHIRsmith comparisons. The secondary target remains at least 3x FHIRsmith throughput on the agreed priority suite without exceeding the agreed p99 limit or reducing response semantics. Neither overall target is achieved. The original scorer/protocol is now pinned and reproducible; do not change weights or omit slow scenarios to inflate results.

The user reports approximately 12,000 RPS for complex CRUD operations in OctoFHIR. Exceeding that throughput on simple terminology lookup/validation is a product target to verify on a defined workload. The batching and prepared-CodeSystem increments now exceed 12,000 RPS on synthetic lookup, CodeSystem/ValueSet validation, subsumption and forward translation at 50 VU. Single-client, clinical and mixed workloads still need their own evidence; these are not general capacity guarantees. Complex expansions require separate targets.

Priority workloads: finite and filtered ValueSet membership, LOINC lookup/search, UCUM, paginated expansion, negative results, different versions and languages. Add large local code systems, inline CodeSystem/ValueSet inputs, imported sets and cold plan preparation. Do not limit comparisons to repeatedly querying one code.

Optimization order follows profiling:

1. Property and membership indexes; intersections/unions instead of repeated linear scans.
2. Fingerprints and prepared plans instead of repeated JSON serialization.
3. Assemble the requested result page without copying the entire source resource.
4. Specialized search with defined normalization and sorting rules.
5. Batch processing and bounded parallelism for expensive CPU work.

Pin both server source revisions, content versions, CPU/RAM limits, response parameters, warmup and cache mode. Collect CPU, RSS, startup, import and snapshot-update costs alongside p50/p95/p99 and req/s. Use both closed concurrency and fixed arrival rates, accounting for queues and rejections. Mixed workloads verify that a heavy expansion does not block lookup/validation.

Acceptance: published reproducible artifacts demonstrate the target on the agreed suite, with no hidden response errors under load or unexplained memory growth.

## Milestone 5: Operational Readiness

Basic limits should appear while building the engine; this milestone completes the operational contract.

- Limits for request size, nesting, result volume, memory, queues and operation cost.
- Cancellation on client disconnect, execution budgets and understandable overload responses.
- Database and active-snapshot readiness; operation, content-version, cache-hit and error metrics.
- Documented startup/migration workflow, container deployment, backup and recovery of the product's own storage.
- Explicit lifecycle and routing between instances for temporary client resources. `$closure` requires separate persistent state if included in the declared contract.
- Installation, import and updates through CLI/UI; public terminology CRUD is not a first-release requirement.

Acceptance: a new instance deploys from the documentation, restores content and passes smoke/conformance checks; updates do not interrupt or silently change in-flight operations.

## Milestone 6: Branded Public UI and Standalone Administration

The first UI/admin increment is implemented. See [the implementation and validation report](UI_ADMIN_IMPLEMENTATION_REPORT.md) and [the continuation checklist](NEXT_SESSION_UI.md).

- React 19 replaces SolidJS and directly consumes the actual `@octofhir/ui-kit` from server-rs commit `5a17d7428e8da253fcd87375e45b41c89ff59997`, selecting `packages/ui-kit` through pnpm's fixed Git subdirectory selector. Node/pnpm and the lockfile are pinned; a clean exported checkout installs/builds without a sibling checkout. Consumer alias resolution fixes the kit's concrete `#/…` packaging defect without copying its components.
- Public overview, three catalogs, filtered pagination, projection concept pages, canonical/business-version selection, resource details/FHIR JSON and supported GET/POST terminology tools remain anonymous. The shell, inputs, tables, dialogs, typography and both themes use the real kit. A new mascot uses the existing squid/sombrero identity in the marigold/ink family.
- Separate `/admin` and `/api/admin` areas use standalone Argon2id bootstrap credentials, persistent expiring/revocable HttpOnly cookie sessions, Origin/CSRF checks and a login retry budget. Mutation authorization precedes body/staging/job effects. Public terminology hot paths have no new auth queries.
- Streaming single-resource R4 JSON and bounded local R4 FHIR `.tgz` package uploads support CodeSystem, ValueSet and ConceptMap with inspection, a bounded persistent queue, cancellation and restart recovery. [The package increment](PACKAGE_IMPORT_REPORT.md) adds aggregate archive limits and all-resource atomic activation without downloading dependencies. Shared `term-squid-ingestion` publishes metadata/projections and job success atomically, preserving immutable versions, idempotence and generation freshness. CLI commands retain their formats and native manifest workflows.
- Frontend-before-RustEmbed ordering and artifact paths are fixed in just/CI/release recipes. CI is configured to exercise the embedded release, browser workflows, process restart and unchanged local semantic gates.

This increment does not implement Bundle/arbitrary archive import, package dependency resolution, RF2/LOINC multifile upload, general IAM/OIDC, content editing/deletion, RxNorm or missing LOINC accessories. It does not establish complete FHIR conformance, clinical coverage or a new performance/composite score. One configured admin worker owns a schema; public replicas may remain unconfigured. [The pinned FHIRsmith comparison](FHIRSMITH_UI_ADMIN_COMPARISON.md) distinguishes its public terminology UI and startup library ingestion from this authenticated online upload/job workflow.

## Later Development

R5, additional terminologies, full ECL, authoring, syndication and advanced mappings follow the priority contract. The benchmark-required SNOMED release ingestion and explicitly tested implicit/filter subset are now in the current composite workstream, not postponed here. OctoFHIR integration is considered separately through the standard API; moving term-squid data into the FHIR server database is outside the plan.

The next working increment after introducing benchmarks fixes ValueSet membership and expansion while extracting a typed resolver and compose planner into the library. Preserve preflight results and comparable measurements at each step so speed improvements cannot hide behavior changes.
