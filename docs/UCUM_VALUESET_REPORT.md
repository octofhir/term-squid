# Explicit UCUM ValueSet composition

November 5, 2026. Finite ValueSets can now select explicit UCUM expressions through the real bundled grammar. This extends the [canonical lookup/validation provider](UCUM_PROVIDER_REPORT.md); it does not enumerate all possible units or claim complete ecosystem conformance.

## Supported behavior

Stored, instance, inline, imported and request-local ValueSets support `compose.include` and `compose.exclude` selections with `system: http://unitsofmeasure.org` and a nonempty explicit `concept` list. The selected bundled edition is **2.2**, supplied by the locked **octofhir-ucum 0.9.3** library. Codes are case-sensitive and expressions are evaluated by that library, including compound, annotated and special units. No common-units whitelist or replacement parser is introduced.

Expansion, pagination, exact totals, text filtering and membership use the existing finite engine. Other finite CodeSystems can occur in the same composition. Imported intersections, exclusions, canonical/business versions, retained plans and cross-process generation observations retain their normal semantics. An invalid explicit expression fails the operation; it never silently disappears from a partial successful expansion.

Displays originate from the provider, subject to the existing explicit ValueSet display rule. Stored supplements can add real localized designations/properties; only codes participating in the bounded projection are merged. An unrelated valid unit in a supplement does not turn the ValueSet into a wider enumeration. Language, designation selection, abstract/inactive policies and historical supplement selection use the same preparation snapshot. The bundled revision date comes from the library's `UCUM_REVISION_DATE` (**2024-06-17**); earlier historical selection fails explicitly.

Canonical bundled-2.2 capabilities cannot inherit whole-system filters or an is-a hierarchy from a separately imported resource instance. Canonical UCUM subsumption fails explicitly; stored instance lookup retains its own selected content.

Expansion provenance reports the actual `http://unitsofmeasure.org|2.2` dependency. The internal source is a partial grammar projection, never a published complete CodeSystem. No database resource, staging file or job is created by the read. Negative membership for an unlisted valid unit does not claim that the unit is unknown to UCUM.

The reusable API `UcumProvider::prepare_explicit(CodeSystem)` validates the bundled identity and each real expression, supplies provider displays, applies strict R4 finite metadata checks and marks the result as an explicit projection. The engine itself prevents using that projection for a whole-system or property-filter expansion. Its compact code index retains partial coverage. The independent core consumer exercises this API without HTTP, SQL, TLS or Tokio dependencies.

## Bounds and limitations

- At most **1,024 distinct explicit expressions per composition**, including imported selections; at most **1,024 bytes per expression**. Existing resource, input-byte, nesting, composition-work, page, response, queue and deadline limits still apply.
- Whole UCUM selections, whole-system exclusions and compose property filters are explicitly unavailable. Infinite grammar expansion cannot be substituted with a short finite list. `excludeSystem` can still remove UCUM members from an otherwise supported explicit finite expansion.
- UCUM implicit ValueSet URI conventions, comparability filters, conversion HTTP operations and mixed native/finite provider composition remain separate work.
- Unversioned or explicitly 2.2 selections use the bundled provider. Other explicit business editions retain the existing stored-resource resolution and unavailable-edition errors; they do not silently fall back to 2.2.
- Preparation runs in the existing bounded CPU work path. Cancelling a request cannot forcibly interrupt one bounded core call; its permit remains owned until computation returns.

## Example

Publish this resource through the authenticated upload/job workflow, or supply it in the `valueSet` parameter of public POST `$expand`/`$validate-code`:

```json
{
  "resourceType": "ValueSet",
  "url": "urn:example:units",
  "version": "1",
  "status": "active",
  "compose": {
    "include": [{
      "system": "http://unitsofmeasure.org",
      "concept": [{"code": "mg"}, {"code": "mg/dL"}]
    }]
  }
}
```

Expansion contains those two units with business version 2.2. Membership accepts `mg` and rejects the unlisted `kg` without asserting that `kg` is an invalid UCUM expression. Supported terminology POST reads remain anonymous.

## Validation and upgrade

Two core tests cover real expressions, provider identity, display generation, projection restrictions, partial code knowledge and bounds. Four PostgreSQL/public-router scenarios cover stored/instance/inline/imported/contextual forms, finite composition, supplementary languages, exact pages, explicit editions, historical selection, commit/rollback, cache parity and absence of published synthetic CodeSystems. The production browser/API scenario uploads a real unit ValueSet, starts and observes its import job, logs out and checks anonymous expansion/membership.

See [REQUEST_CONTEXT_REPORT.md](REQUEST_CONTEXT_REPORT.md) for the final combined workspace, embedded-production, clean-build and pinned local FHIRsmith evidence. No new migration or dependency is required; apply existing migrations through `20261104000006_supplements` and rebuild frontend before the embedded backend.
