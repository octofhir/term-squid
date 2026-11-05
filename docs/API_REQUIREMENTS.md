# FHIR R4 API requirements matrix

Updated November 5, 2026. The user-confirmed boundary is the full term-squid product API, including all inputs of the six declared R4 operations. Closure/find-matches, general public authoring and new FHIR adapters are outside this boundary. Provider and conformance-registry bounds remain explicit; unsupported input errors are not implementation evidence.

The implemented structural contract contains **70 inputs** in [R4_API_CONTRACT.json](R4_API_CONTRACT.json), transcribed from pinned official OperationDefinitions with source fingerprints, output structures and positive acceptance-test references. [API_IMPLEMENTATION_REPORT.md](API_IMPLEMENTATION_REPORT.md) documents behavior, limits, upgrades and delivery evidence. That API-delivery checkpoint passed 147 Rust/PostgreSQL tests, 23 embedded production browser/API scenarios, 29 harness tests and seven unchanged local FHIR gates. The latest [request-context report](REQUEST_CONTEXT_REPORT.md) records 206 Rust/PostgreSQL tests, 25 embedded browser/API scenarios and 38 harness tests, while the [independent gap audit](ECOSYSTEM_GAP_AUDIT.md) keeps general-mode failures explicit.

| Contract | Implemented behavior | Acceptance evidence |
| --- | --- | --- |
| CodeSystem lookup, 7 inputs | Primitive/Coding, canonical/version, date, displayLanguage, repeated property selection; real designation use and typed definition/property output; retained instance extension | Coding/cache/SQL/UCUM tests; rich finite/native lookup scenarios; supplemental language/property scenarios |
| CodeSystem validation, 10 inputs | Primitive/Coding/CodeableConcept, URL/version, inline CodeSystem, date, display and language, abstract policy; any-valid alternatives on one snapshot | Stored/instance/inline, complete/fragment/case, unknown alternatives, UCUM and concurrent publication scenarios |
| Subsumption, 6 inputs | Primitive or Coding A/B, selected system/version, four outcomes, typed system inference and instance assertions | All outcomes, mixed forms, version/instance and cross-system indeterminate scenarios |
| ValueSet expansion, 21 inputs | Stored/instance/inline/context, version, filter/date, paging, definition/designations/use-language selectors, lifecycle/UI/post-coordination policy, nested results, system exclusions and transitive default/check/force versions | Counts and pages after filtering/localization; native projection reads, finite hierarchy, override/error cases, real production uploads |
| ValueSet validation, 13 inputs | Primitive/Coding/CodeableConcept, independent system/ValueSet versions, inline/context, date/language/display/abstract; unique-system inference | Stored/instance/inline, mixed known/unknown, native, context/date/localization and concurrent snapshot scenarios |
| Translation, 13 inputs | Primitive/Coding/CodeableConcept, inline/map version, map discovery, source/target ValueSet, targetsystem, dependency/product, reverse, provenance and bounded unmapped rules | Positive dependency/product and reverse cases; fixed/other-map/cycle cases; real scope filtering/discovery; production imported maps |
| CodeSystem supplements | Additional designations/properties retain base coding identity; finite filters and localized native/UCUM reads; immutable activation/freshness; no new/overridden codes | Supplemental package dependency ordering/rollback; prepared negative-to-positive and version update; finite/native/UCUM localization |
| HTTP | Strict typed/cardinality/exclusive-form admission, repeated query values, JSON Accept/Content-Type/_format negotiation and FHIR 404/405/errors | Database-independent invalid-request/protocol scenarios and embedded HTTP tests |
| Metadata | CapabilityStatement, standard terminology metadata mode, fresh installed content/version/default/provider inventory and six local OperationDefinition resources | Exact pinned input/output facts, current version activation and UCUM inventory assertions |
| Public/admin separation | Anonymous resource reads and supported GET/POST terminology operations; separate authenticated mutation APIs | Existing protected upload/session/revocation/origin/CSRF and production browser scenarios remain mandatory |
| Build/embedding | Fixed Git subdirectory UI-kit dependency, locked frontend-first embedded build | Clean source export install/build and embedded production validation |

The six operation input counts total 70; expansion has 21, not 22. The old single-Coding report remains historical evidence, not the current API inventory.

The subsequent [official ecosystem `case` increment](ECOSYSTEM_R4_REPORT.md) adds independent selected R4 evidence, numeric transport `nocache` compatibility and structured ValueSet Coding context/normalization/diagnostics. It preserves the seventy operation inputs and their bounded semantics. The selected preloaded profile passes six cases in both retention modes; original `tx-resource` setup and complete general-mode certification remain outside that checkpoint. That historical Rust/PostgreSQL regression count is 150, zero ignored.

## Policies that clients must retain

- R4 4.0.1 is verified. R5/R6 aliases do not establish release support.
- Missing/unavailable/unsupported knowledge is distinct from a definitive invalid-code result. No unknown clinical correspondence is invented for cross-system subsumption.
- Canonical/business versions and resource UUIDs remain separate. Typed instance assertions cannot override selected metadata. The legacy subsumption top-level version remains an instance compatibility exception.
- A local context is a ValueSet canonical or `ValueSet/{uuid}`; incoming/outgoing resolve the same binding. No StructureDefinition conformance registry or remote context fetch is provisioned.
- Historical selection uses definition/release dates, or a caller-selected immutable edition. It does not relabel updated_at as a clinical date. Missing historical evidence fails explicitly.
- Supplements cannot act as Coding.system, introduce new codes or override existing values. Base systems precede supplements during atomic package publication while global canonical lock ordering is preserved. Apply migration `20261104000006_supplements`.
- Native RF2/LOINC content=not-present still has real projected concepts. Native clinical expression/accessory/refset and UCUM ValueSet enumeration bounds remain documented provider limits. [Explicit finite UCUM selections](UCUM_VALUESET_REPORT.md) now use the real grammar for expansion/membership while preventing whole-grammar enumeration. They are not clinical completeness claims.
- Public reads do not query sessions or authorization tables. Complex operations retain read-only snapshots, work/admission limits, deadlines and cancellation accounting. No benchmark assertion, pool weight or scorer rule was relaxed.

## Primary references

- [R4 CodeSystem lookup](https://hl7.org/fhir/R4/codesystem-operation-lookup.html), [validation](https://hl7.org/fhir/R4/codesystem-operation-validate-code.html), [subsumption](https://hl7.org/fhir/R4/codesystem-operation-subsumes.html).
- [R4 ValueSet expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html), [validation](https://hl7.org/fhir/R4/valueset-operation-validate-code.html), [ConceptMap translation](https://hl7.org/fhir/R4/conceptmap-operation-translate.html).
- [R4 invocation](https://hl7.org/fhir/R4/operations.html), [HTTP](https://hl7.org/fhir/R4/http.html), [supplements](https://hl7.org/fhir/R4/codesystem.html#supplements), [terminology metadata](https://hl7.org/fhir/R4/terminologycapabilities.html).

The subsequent [request-context increment](REQUEST_CONTEXT_REPORT.md) adds the bounded `tx-resource` protocol to all six public POST operations without changing the seventy normative R4 inputs. Original setup now passes before import in both retention modes. Broader general-mode ecosystem coverage remains incomplete; do not equate declared input coverage with full release conformance.

The same increment adds container-scoped local ValueSet references, actual designation validation, typed bounded finite filters and compact same-snapshot code knowledge. ValueSet validation accepts repeated system-version/check-system-version/force-system-version client extensions. Both validation operations accept the explicit lenient-display-validation boolean policy; it can warn only for an incorrect display, never for unknown codes, nonmembership, unavailable editions or forbidden abstract codes. Public operation POSTs retain anonymous access with admission before body parsing and an overall bounded deadline. See the request-context report for exact limits and evidence.

Required ValueSet supplement declarations are checked against actual active overlays in the selected snapshot, including imported/contained/temporary/native definitions. A missing or mismatched required edition fails explicitly; forcing an older supplement edition is not implemented. The [generated finite model](FINITE_DIFFERENTIAL_REPORT.md) and independent graph-admission tests add positive/negative evidence without replacing original goldens or claiming unrestricted FHIR support.

The latest post-timing review retains a known non-ASCII comparison boundary between SQL-backed stored lookup and the independent request-local engine; see [the exact U+0130 reproduction](REQUEST_CONTEXT_REPORT.md#remaining-unicode-comparison-boundary). Top-level input coverage is not proof of equivalent behavior for every Unicode mapping or unrestricted FHIR conformance.
