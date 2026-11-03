# Built-in UCUM grammar provider

November 3, 2026. term-squid now reuses **`octofhir-ucum = 0.9.1`**, pinned to the same version selected by the local FHIRPath manifest. It is a registry dependency, not a sibling-checkout dependency. Default features are disabled: no CLI, FHIR serialization, regex or fuzzy-search feature is needed by this adapter.

## Implemented contract

- The reusable Rust API is `term_squid_core::ucum::UcumProvider::{lookup, comparable}`. Parsing, semantic evaluation, unit names and comparability come from the library; no custom UCUM grammar, arithmetic or finite whitelist is introduced.
- The immutable bundled model is UCUM **2.2**, revision date June 17, 2024. `ucum-essence.xml` SHA-256: `dfccea1b5dc284245ebae97edd1dc03c45864da4e87df55bc9851797b4fd0b61`. The crate checksum and transitive versions are recorded in `Cargo.lock`.
- Canonical `CodeSystem/$lookup` and `CodeSystem/$validate-code` GET/POST use this provider for `http://unitsofmeasure.org`, unversioned or explicitly `2.2`, including versioned canonical strings. These operations need no SQL observation or imported unit table. Server startup still requires PostgreSQL for the rest of the product.
- Atomic, prefixed, compound, annotated and special expressions are evaluated with case-sensitive UCUM rules, including `mg`, `mg/dL`, `kg.m/s2`, `mm[Hg]`, `Cel`, `[degF]` and `{count}/min`.
- Lookup returns the library-generated display and model version. Invalid codes return HTTP 404 for lookup and `result=false` for validation. Existing exact-display matching remains; this is not complete language/designation/display-alias conformance.
- Expressions are limited to 1,024 bytes; the library's nesting/precision limits remain. Exceeded evaluation limits return explicit errors rather than pretending a valid but unevaluable code is invalid. Empty FHIR code input is not treated as UCUM's empty unity term.
- Other explicitly selected versions retain stored resolution; missing releases return HTTP 404. Instance requests retain their selected database resource and do not silently switch to the built-in default. Version conflicts remain errors.
- TerminologyCapabilities declares the default compositional UCUM 2.2 provider. `comparable` is a Rust library API, not a newly invented FHIR HTTP operation. Invalid comparability operands are errors, not a successful `false` result for two valid units.

This increment does **not** implement UCUM implicit ValueSet resolution, finite compose/provider integration, arbitrary unit enumeration, HTTP conversion endpoints or complete ecosystem conformance. The existing finite compose evaluator is unchanged; a full UCUM grammar cannot be represented by a short common-units expansion.

References: [crate 0.9.1](https://crates.io/crates/octofhir-ucum/0.9.1), [library source](https://github.com/octofhir/ucum-rs). API behavior was checked against the downloaded pinned source (`src/lib.rs`, `parser.rs`, `evaluator.rs`, `error.rs`), not an unversioned documentation search result. The dependency is Apache-2.0 licensed; no source was copied into this repository.

## Verification

All **57 Rust tests pass**, including fourteen PostgreSQL scenarios, four provider tests and two canonical HTTP/stored-context tests. Clippy with warnings denied and formatting pass. Provider tests distinguish invalid/case-sensitive codes from resource/version errors, exercise compound/special units, and compare compatible/incompatible/invalid operands. A lazy unavailable PostgreSQL pool in the canonical router test proves that the built-in operations do not issue SQL. Separate database tests preserve explicit stored versions and instance identity.

Release SHA-256: `1781d4bb8cc6ac96bea22825ec13d9c72b7817f1236fd66ee55e75d6b7254494`. Real local release HTTP smoke passes **8/8 UCUM checks** and **6/6 inline checks**. The unchanged synthetic preflight passes 7/7. Original upstream clinical preflight remains 2/20 (seven failures, eleven skips), because the clinical corpus and related semantics are still incomplete. The original Round 0 suite has no standalone UCUM scenario; this feature is not presented as a composite-score increase.

Final artifacts: `.benchmarks/profiles/inline-ucum-final/`, `.benchmarks/results/inline-ucum-local-final/`, `.benchmarks/results/inline-ucum-upstream-final/`. The earlier UCUM-only verification remains under `.benchmarks/profiles/ucum-provider/`. No production operations or load tests were performed. No performance superiority is claimed from these functional checks.

After starting the updated server with `just bench-server`:

```sh
curl --get 'http://127.0.0.1:18081/r4/CodeSystem/$lookup' \
  --data-urlencode 'system=http://unitsofmeasure.org' \
  --data-urlencode 'code=mg/dL' --data-urlencode 'version=2.2'
curl --get 'http://127.0.0.1:18081/r4/CodeSystem/$validate-code' \
  --data-urlencode 'system=http://unitsofmeasure.org' \
  --data-urlencode 'code=not-a-unit'
```
