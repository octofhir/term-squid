# FHIR R4 API requirements matrix

Updated November 4, 2026. The user-confirmed boundary is the full term-squid product API, including all inputs of the six declared R4 operations. Closure/find-matches, general public authoring and new FHIR adapters are outside this boundary. Provider and conformance-registry bounds remain explicit; unsupported input errors are not implementation evidence.

The implemented structural contract contains **70 inputs** in [R4_API_CONTRACT.json](R4_API_CONTRACT.json), transcribed from pinned official OperationDefinitions with source fingerprints, output structures and positive acceptance-test references. [API_IMPLEMENTATION_REPORT.md](API_IMPLEMENTATION_REPORT.md) documents behavior, limits, upgrades and delivery evidence. Validation passes: 147 Rust/PostgreSQL tests, 23 embedded production browser/API scenarios, 29 harness tests and seven unchanged local FHIR gates. The report is the checkpoint authority.

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

## Policies that clients must retain

- R4 4.0.1 is verified. R5/R6 aliases do not establish release support.
- Missing/unavailable/unsupported knowledge is distinct from a definitive invalid-code result. No unknown clinical correspondence is invented for cross-system subsumption.
- Canonical/business versions and resource UUIDs remain separate. Typed instance assertions cannot override selected metadata. The legacy subsumption top-level version remains an instance compatibility exception.
- A local context is a ValueSet canonical or `ValueSet/{uuid}`; incoming/outgoing resolve the same binding. No StructureDefinition conformance registry or remote context fetch is provisioned.
- Historical selection uses definition/release dates, or a caller-selected immutable edition. It does not relabel updated_at as a clinical date. Missing historical evidence fails explicitly.
- Supplements cannot act as Coding.system, introduce new codes or override existing values. Base systems precede supplements during atomic package publication while global canonical lock ordering is preserved. Apply migration `20261104000006_supplements`.
- Native RF2/LOINC content=not-present still has real projected concepts. Native clinical expression/accessory/refset and UCUM ValueSet enumeration bounds remain documented provider limits. They are not clinical completeness claims.
- Public reads do not query sessions or authorization tables. Complex operations retain read-only snapshots, work/admission limits, deadlines and cancellation accounting. No benchmark assertion, pool weight or scorer rule was relaxed.

## Primary references

- [R4 CodeSystem lookup](https://hl7.org/fhir/R4/codesystem-operation-lookup.html), [validation](https://hl7.org/fhir/R4/codesystem-operation-validate-code.html), [subsumption](https://hl7.org/fhir/R4/codesystem-operation-subsumes.html).
- [R4 ValueSet expansion](https://hl7.org/fhir/R4/valueset-operation-expand.html), [validation](https://hl7.org/fhir/R4/valueset-operation-validate-code.html), [ConceptMap translation](https://hl7.org/fhir/R4/conceptmap-operation-translate.html).
- [R4 invocation](https://hl7.org/fhir/R4/operations.html), [HTTP](https://hl7.org/fhir/R4/http.html), [supplements](https://hl7.org/fhir/R4/codesystem.html#supplements), [terminology metadata](https://hl7.org/fhir/R4/terminologycapabilities.html).
