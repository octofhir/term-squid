# term-squid Product Development Plan

Status: implementation started; milestones define the proposed delivery order. Updated November 2, 2026.

term-squid is a standalone terminology product with its own FHIR API, content, storage, CLI and UI. Its internal engine is designed as a Rust library. Embedding it in OctoFHIR or placing terminology data in the OctoFHIR database is outside the current scope.

The first product release prioritizes local code systems, HL7, LOINC and UCUM. FHIR compatibility is required; performance comes from prepared data, indexes, compact representations and execution design.

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

The next increment adds bounded immutable finite plans with transactional cross-process invalidation. See [PREPARED_DATA_REPORT.md](PREPARED_DATA_REPORT.md) for evidence and remaining shared-snapshot/admission work. The subsequent [warm-read increment](WARM_READ_REPORT.md) makes generation checks concurrent while retaining post-commit freshness. The [transport increment](TRANSPORT_PERFORMANCE_REPORT.md) evaluates existing OctoFHIR pool and TCP patterns against the shared local FHIRsmith suite.

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

Acceptance: local preflights pass; selected ecosystem scenarios produce the same correct results with caching enabled and disabled. Contract completeness is defined by a requirements matrix, not just the small local suite.

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
| UCUM | Provider using the existing `octofhir-ucum`: expression grammar, lookup and comparability; a finite common-units list does not replace the full system |
| LOINC | Import the official release: axes, class, status, names, languages, Parts, hierarchy, Answer Lists and Groups; capabilities declare supported properties and filters |

Imports record version, checksum and source. Load data in streams or batches rather than one SQL round trip per concept. An incomplete import never becomes active. Materialize large expansions as needed within memory limits. Clinical source data stays out of Git; version manifests and permitted test fixtures belong in Git.

Acceptance: reproducible content updates and rollback; correct selected HL7/LOINC/UCUM scenarios against pinned data releases.

## Milestone 4: Demonstrated Speedup over FHIRsmith

Performance target: at least 3x throughput on the agreed priority suite without exceeding the agreed p99 limit or reducing response semantics. This is a development target, not an achieved result. Before establishing the priority-operation baseline, fix the aggregation method and acceptable regressions so weights or omitted scenarios cannot inflate the reported speedup.

The user reports approximately 12,000 RPS for complex CRUD operations in OctoFHIR. Exceeding that throughput on simple terminology lookup/validation is a product target to verify on a defined workload. It is not an existing term-squid result, and complex expansions require separate targets.

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

## Later Development

R5, additional terminologies, full SNOMED CT/ECL, authoring, syndication and advanced mappings follow the priority contract. OctoFHIR integration is considered separately through the standard API; moving term-squid data into the FHIR server database is outside the plan.

The next working increment after introducing benchmarks fixes ValueSet membership and expansion while extracting a typed resolver and compose planner into the library. Preserve preflight results and comparable measurements at each step so speed improvements cannot hide behavior changes.
