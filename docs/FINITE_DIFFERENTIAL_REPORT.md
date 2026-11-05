# Generated finite R4 semantic comparison

November 5, 2026. Identical generated request-local CodeSystems and ValueSets exercise the finite product contract on local term-squid and pinned FHIRsmith. This is an independent fixture model and public API correctness check, separate from both the official ecosystem matcher and load/performance scoring.

The 16-seed run checks **208 scenarios**, each with six responses per participant: temporary CodeSystem lookup, a complete flat expansion, positive membership, outside membership, unknown-code membership and a subsequent ordinary lookup proving that the temporary CodeSystem is unavailable outside its request. term-squid passes **208/208 scenarios and 1,248/1,248 response checks**. FHIRsmith passes **104/208 scenarios and 978/1,248 checks**. Positive lookup and released-context checks pass on both in every scenario, so unavailable terminology cannot earn a successful negative by itself.

## Fixture model and checks

Fixtures contain 24–32 concepts in a binary hierarchy with actual scalar properties and inactive flags. Sixteen deterministic seeds vary source attributes, inclusion/exclusion sets, anchors, property presence and case sensitivity. Expected graph relationships use integer heap ancestry independently of the nested FHIR definitions; expected membership uses ordinary set algebra. No production parser/planner is copied into the harness.

The oracle checks the complete code set, total, source system, real source labels and business version when emitted. Equivalent code spelling is accepted only for declared case-insensitive systems; duplicates, wrong versions/labels, missing members and nested output despite `excludeNested=true` fail. Expansion `activeOnly` is evaluated separately from validation membership; `compose.inactive=false` constrains both. Each scenario must include a genuine positive control. The original seven gates and official requests/goldens are unchanged.

| Generated scenario family | term-squid / 16 | FHIRsmith / 16 |
| --- | ---: | ---: |
| enumerated | 16 | 16 |
| import-intersection | 16 | 16 |
| diamond | 16 | 0 |
| is-a | 16 | 8 |
| generalizes | 16 | 0 |
| is-not-a | 16 | 8 |
| code-in | 16 | 8 |
| code-not-in | 16 | 0 |
| regex | 16 | 8 |
| property-equality | 16 | 8 |
| property-exists | 16 | 0 |
| inactive | 16 | 16 |
| compose-inactive | 16 | 16 |

## Observed reference differences

- `generalizes` and `code not-in` return explicit unsupported-filter errors in expansion and validation on this generic provider. Provider filter inventories are optional capabilities; this is not a blanket FHIR conformance ranking.
- Property `exists` expansion returns an empty result while validation reports the filter unsupported for the supplied property.
- Diamond imports/exclusion produce extra expansion members in all sixteen cases while the sampled validation decisions agree. Full requests, expected sets and returned codes are retained for independent investigation.
- Case-insensitive filtered membership fails on selected hierarchy/code/property cases even though the source lookup succeeds and a display is returned. Case-insensitive hierarchy and regex expansion also differ. These are recorded against this exact reference revision/configuration; they are not generalized to another release or provider.

Unsupported/incorrect scenarios receive no speed score. The broader [official audit](ECOSYSTEM_GAP_AUDIT.md) remains **110/689 versus 682/689** with zero ignored cases; these generated expectations do not replace those failures or imply certification. The profiles answer different questions.

## Provenance and reproduction

term-squid binary SHA-256: `6ddb9ad1d0ccb5a62c6a56cbf28ec7d7bc567ddffdbd03dbb1f11d1510cb0ae0`. FHIRsmith: `25a41f87711507789b361480ee4efae42df504d4` / 0.18.2, with only the documented bootstrap R4 core pin. Complete package/synthetic content stays installed, but these scenarios supply their own bounded definitions and require no new import.

Harness SHA-256: `25aae55fd457917b2f0915d2d4d1dd4a46bfa5ad2e122890505a5cebf831a5c7`. Full generated fixtures, requests/responses and manifests remain under `.benchmarks/overnight-20261105/finite206-model16/`. Four harness tests independently check fixed expected sets, active/inactive policy separation, all 64 available seeds and rejection of malformed responses. The complete harness now passes 38 tests / 40,251 assertions.

```sh
bun test benchmarks
bun benchmarks/finite-differential.ts \
  --left http://127.0.0.1:18081/r4 --right http://127.0.0.1:17003/tx/r4 \
  --seeds 16 --output .benchmarks/UNIQUE-finite-model \
  --left-revision BACKEND_REVISION --right-revision 25a41f87711507789b361480ee4efae42df504d4
```

Use fresh local processes and a fresh output directory. The command retains reference failures and exits unsuccessfully for a failed left-side expectation. Run it outside timing windows. Raw fixtures/logs stay gitignored and do not travel with the repository.
