# R4 Coding input normalization and protocol errors

November 4, 2026. This implements the next P0 input-contract increment from [PRODUCT_PLAN.md](PRODUCT_PLAN.md). See [API_REQUIREMENTS.md](API_REQUIREMENTS.md) for implemented and outstanding API requirements. The base revision is `b426651ccb2377cecfe6c6d75b7cf1985168f658`, with all accumulated UI/admin/package and current API changes preserved in the working tree. No commit, push, publication or deployment is part of this work.

## Implemented routes

All existing GET/POST type and instance routes for these operation families remain public:

- `/r4/CodeSystem/$lookup` and `/r4/CodeSystem/{id}/$lookup`.
- `/r4/CodeSystem/$validate-code` and `/r4/CodeSystem/{id}/$validate-code`.
- `/r4/ValueSet/$validate-code` and `/r4/ValueSet/{id}/$validate-code`.

POST now accepts one `coding` parameter containing `valueCoding` with required system/code, optional business version/display and optional boolean `userSelected`. Lookup uses the code identity and returns its registered details; validation also checks the supplied display using the existing evaluator. The existing exact preferred-display policy is preserved, not promoted to full designation/language conformance.

Example against the bundled UCUM provider:

```sh
curl -sS 'http://127.0.0.1:8081/r4/CodeSystem/$lookup' \
  -H 'Content-Type: application/fhir+json' \
  --data '{"resourceType":"Parameters","parameter":[{"name":"coding","valueCoding":{"system":"http://unitsofmeasure.org","version":"2.2","code":"mg"}}]}'
```

For type-level CodeSystem validation of a Coding, include `{"name":"url","valueUri":"http://unitsofmeasure.org"}`. Primitive requests retain the legacy `system` URL selector and supported string aliases. A redundant lookup `url` constraint, used by existing tests/clients, is checked against the supplied system instead of silently discarded.

Primitive code/system/display and complex Coding are exclusive forms. A consistent separate version can accompany a Coding; conflicts fail. CodeSystem Coding.system must agree with its selected target. Instance routes check asserted system/business version against the actual selected resource metadata. ValueSet `Coding.version` supplies the code-system context; it never selects the ValueSet business version. Stored `valueSetVersion`, inline resource identity and instance selection retain their existing separate rules. Explicit dependency versions retain precedence over a default system context.

The shared decoder rejects duplicate/unknown parameters, wrong value types, mixed resource/value/part representations, missing code/system, empty/oversized admitted text, conflicting selectors and unsupported CodeableConcept/options. Top-level POST parameters are bounded at 64, text at 4,096 UTF-8 bytes and JSON bodies at the existing 2 MiB. Inline resources keep their existing 256-KiB and graph/dependency/provider limits. Coding extensions/primitive metadata are rejected explicitly; full FHIR profile/lexical validation is not claimed.

## Errors and representation

The touched operation handlers use common FHIR extractors. Invalid JSON/data/query/UUID input returns HTTP 400 OperationOutcome; unsupported posted media type returns 415, oversized JSON 413, and unsupported `_format` 406. Supported JSON format spellings are `json`, `application/json` and `application/fhir+json`; these handlers return the FHIR JSON representation. POST clinical parameters belong in the Parameters body; only `_format` is admitted in the query, preventing silently ignored conflicting query inputs.

Lookup/validation success and AppError responses now use `application/fhir+json`. An invalid code in known complete content still returns the existing semantic result, rather than being confused with an unavailable version, incomplete source or unsupported native code family. Existing canonical/core-expression 422 behavior, including UCUM version conflicts and unloaded LOINC families, is preserved. Internal/database errors remain sanitized.

This is a bounded JSON contract for these operation families. Full Accept negotiation and the same extraction coverage on expansion/subsumption/translation are follow-up work; this report does not claim complete HTTP or FHIR conformance.

## Snapshot and engine preservation

`api/coded_input.rs` normalizes the supported forms before store dispatch. No parser, source resource, credentials or session state is added to the engine cache keys. Normalized inputs go through the existing finite, native and bundled UCUM providers.

`ResolvedConcept` now includes its selected canonical URL. Prepared metadata and SQL fallback fetch it with the existing concept snapshot; native projections follow the same route. An instance assertion therefore requires no separate resource/auth database query and cannot compare a concept from one generation with independently selected identity metadata. Metadata admission/accounting includes the URL. The immutable cold metadata is boxed once; the bounded per-read result stays inline to avoid adding a box allocation to every warm read. That read enum has a documented local `large_enum_variant` lint exception; workspace Clippy still runs with warnings denied. No semantic assertion or budget was relaxed.

The existing generation observer, cold repeatable-read snapshots, cache budgets, activation triggers and rollback semantics remain. New tests cover identity/version/display updates before and after commit, rollback, warm preparation and forced SQL fallback. UCUM Coding lookup/validation is tested with an intentionally unreachable database to prove those calls do not acquire auth/storage dependencies. Native LOINC Coding requests select the correct older release. The production package browser flow primes a negative Coding lookup, activates its source package, then verifies the same request succeeds.

CapabilityStatement now lists the six existing operations with official R4 definition canonicals and text describing the actual supported parameter subsets. TerminologyCapabilities documents Coding scope while retaining the actual bundled UCUM declaration. R5/R6 remain unverified aliases; no new adapter is claimed.

## Upgrade and validation

No new migration or dependency/toolchain update is required. All existing migrations, including admin/package migrations, remain required for the full application. Build the frontend before compiling RustEmbed, using the existing locked Node 24.16.0 / pnpm 10.11.0 / Git UI-kit revision `5a17d7428e8da253fcd87375e45b41c89ff59997` configuration.

All final checks passed locally. Raw logs, source exports, binaries and fixtures stay under gitignored `.benchmarks/api-coding/`; generated uploads/screenshots remain under `.benchmarks/ui-admin/`.

| Validation | Evidence under `.benchmarks/api-coding/` |
| --- | --- |
| Formatting | `fmt-final.log`, passed |
| Workspace Clippy, all targets, `-D warnings` | `clippy-final.log`, passed with the documented local inline-result lint decision |
| Workspace tests including PostgreSQL integration | **127 passed**, none ignored: backend 61, CLI 15, core 42, ingestion 9; `rust-delivery.log` |
| Frontend type checks / production build / lint | Passed; build includes `tsc --noEmit`; `frontend-build.log`, `frontend-lint.log` |
| Frontend unit tests | **2 passed**, `frontend-unit.log` |
| Embedded release browser/API tests | **22 passed**, `embedded-e2e.log`; includes typed public Coding, strict errors, metadata, public/admin isolation, real JSON/package activation and source-negative freshness |
| Actual importer crash/restart | JSON and multi-resource package interruption/recovery passed in the same embedded driver; earlier package writes never became published |
| Benchmark harness | **29 passed**, 34,217 assertions; `harness.log` |
| Unchanged local FHIR gates | **7/7**; `.benchmarks/results/ui-admin-1793805809928352000/term-squid`, fixture SHA-256 `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017` |
| Clean install/build | `clean-install.log`, `clean-frontend.log`, `clean-rust.log`: empty node_modules, frozen Git dependency, no supplied static assets or sibling server-rs, frontend before locked offline Cargo release |
| Live preview | Updated immutable artifact on port 8081; the UI proxy on 5173 returned 200 FHIR JSON for anonymous UCUM Coding lookup/validation, configured session discovery and the new R4 operation metadata |

All **154** build-source files match the clean export; fingerprints are recorded in `delivery-manifest.json`. The validated preview backend SHA-256 is `246eb3fe449254c96b13766bbe71c16a560b72848aa9dd41694d2ecbb0b1662f`. The frozen clean install reused the normal pnpm content-addressed cache; the prior UI increment separately checked a fresh store. The standard Vite chunk-size warning remains unchanged.

Existing assertions were preserved. The first regression run exposed a legacy redundant lookup `url` query used by native LOINC tests; the implementation now validates that constraint while retaining its 422 unsupported-family behavior. Existing UCUM canonical conflict status assertions also remain unchanged. The new schema/semantic cases add assertions without weakening old ones. No load/performance comparison or clinical composite was attempted for this adapter change.

## Remaining limitations

CodeableConcept, inline CodeSystem input, language/date/abstract/supplement options, arbitrary property selection, richer lookup designation/lifecycle representation and system inference remain incomplete. Expansion/subsumption/translation still need the same strict adapter work and their own typed semantics. Metadata still needs a full actual content/version/filter inventory and broader standard discovery behavior. Clinical corpus gaps, native provider limits and the R4 target/benchmark compatibility decision remain as documented in the existing reports. No clinical coverage, full R4 conformance or new performance/composite advantage is claimed.
