# Resume development on another device

## Goal and current state

The primary KPI is **first place by the unchanged HealthSamurai TX Benchmark composite**, with direct local Termbox/FHIRsmith comparisons. Do not replace it with synthetic peaks, changed weights, sampled-only clinical imports or historical different-host readings.

The commit containing this document includes the code, benchmark integration, reports and plans. Before switching devices, make sure that commit is available in the remote or another explicit transfer. A local commit alone cannot be fetched from another computer; pushing requires the user's instruction.

Read in order:

1. `AGENTS.md` and this file.
2. `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`.
3. `docs/HEALTHSAMURAI_BASELINE.md`, `benchmarks/README.md`.
4. `docs/SNOMED_RF2_REPORT.md`, `docs/INLINE_VALUESET_REPORT.md`, `docs/UCUM_PROVIDER_REPORT.md`.
5. `docs/HTS_FEATURE_GAPS.md` and the relevant performance report before changing the measured engine.

Implemented:

- Prepared lookup/validation, finite ValueSets, ConceptMaps and hierarchies with bounded retention and fresh cross-process generation observations. No TTL/stale-error fallback; pool default 32.
- Inline ValueSet expansion/membership through the finite engine, content-fingerprinted request-local keys, no persistence and explicit unsupported input errors.
- Ready `octofhir-ucum = 0.9.1`, model 2.2: canonical grammar lookup/CodeSystem validation without SQL and Rust comparability. UCUM ValueSet integration remains incomplete.
- Native checksummed RF2 Snapshot import, atomic activation, selected-dialect display projection and bounded active inferred subsumption. Native ValueSet/implicit/filter/ECL behavior is still unsupported.
- Original Round 0 scorer/pools/protocol and a local graph site. The five historical scores reproduce; no comparable term-squid clinical composite is claimed. Existing-corpus preflight is still 2/20, not an ongoing crashed load test.

Plans and trackers are committed under `vibe/healthsamurai-*`, `vibe/inline-valueset-*`, `vibe/ucum-provider-*` and `vibe/snomed-rf2-*`. A completed tracker covers that increment only, not completion of the composite/product goal.

## Clean-machine setup

This is standalone Git, not Arcadia. Preserve any working-tree changes; do not reset/rebase/commit/push automatically.

Install current stable Rust, Git, Docker Compose, Bun and k6; Node 24 is needed for the pinned FHIRsmith reference. The measured machine used Bun 1.4.4, k6 2.3.0 and Node 24.16.0. Do not assume old absolute tool paths or identical hardware. `just` provides the recipes. The first build needs registry access; use Cargo offline flags only after fetching dependencies.

From the repository root:

```sh
git status --short
just bench-setup
# Uses the repository CLI's migrate command; no separate sqlx-cli is required.
# Also load public R4 core to reproduce the two package-based upstream probes:
CARGO_TARGET_DIR=.benchmarks/target cargo run --release --bin term-squid-cli -- \
  --database-url 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  import hl7.fhir.r4.core --version 4.0.1 -y

TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
bun test benchmarks

# Separate terminal:
just bench-server
# Another terminal:
just bench-preflight --strict
just bench-reference
just bench-upstream --preflight-only --run-id rf2-final-upstream
just bench-ui
```

Open **http://127.0.0.1:4100/tx-benchmark/local/**. The site fetches the pinned upstream source and installs its frozen site dependencies. On a clean machine, the published Round 0 charts are available; old local raw runs are absent, so the local page reports missing charts instead of inventing data. Static build uses `.benchmarks/ui-build`, separate from the dev server caches.

Apply **all** migrations, including `20261103000001_snomed_rf2`, before starting the updated backend. Startup does not apply them. Do not load native RF2 data while running an older backend. Never copy one compiled binary across OS/architectures; rebuild locally. Stop a running binary before replacing its file, or use a fresh immutable artifact filename.

## What Git does not contain

`.benchmarks/` is ignored: no database volumes, clinical archives, extracted files, secrets, generated fixtures, raw timing/profile JSON, binaries, upstream/FHIRsmith/HTS checkouts, Node/k6/browser installs or generated site dependencies. Historical reports contain summaries and fingerprints, not portable copies of those artifacts.

Recreate the synthetic database with the recipes. The two previously passing upstream probes also require R4 core 4.0.1; synthetic-only setup does not reproduce that coverage. If its registry download is unavailable, import an independently obtained official local `.tgz` archive rather than substituting a different version. Obtain clinical files locally under their content terms. RF2 importer source-file hashes and the benchmark corpus/archive manifest are different provenance layers; record both correctly. Never mark clinical corpus attestation complete merely because synthetic setup passed.

For the clinical corpus, use the exact versions in `benchmarks/upstream.json`. Start the SNOMED import from `benchmarks/snomed-rf2.template.json` and the Round 0 corpus attestation from `benchmarks/round0-dataset.template.json`. Both templates intentionally fail unchanged. Native loader accepts complete merged Snapshots, not standalone extension/Full/Delta files. Native LOINC/RxNorm import is not implemented yet.

The prior pinned local FHIRsmith setup is documented in the performance reports: source `25a41f87711507789b361480ee4efae42df504d4`, version 0.18.2, with recorded bootstrap-only package/loopback changes. Do not substitute its results for published FHIRsmith 0.8.6. Local Termbox must be provisioned separately and pinned by image digest/revision. The production tx5 metadata identified a different version and was not load-tested.

## Next work: native clinical ValueSets

1. Start at `crates/backend/src/store/postgres.rs::compose_snapshot`: it deliberately rejects `native_provider` dependencies. `ExpandedValueSet` currently owns a fully materialized finite plan, capped at 100,000 concepts. Do not turn an absent JSON `concept` array into a successful empty native set or just raise all cache limits.
2. Implement a bounded native SNOMED provider for implicit whole-system/hierarchy sets and inline compose using `snomed_relationships`, concept projections and recorded release context. Expansion must produce filtered count/offset pages without cloning/materializing the full release; membership must use the same semantics. Keep source/version/instance selection and one-operation snapshot consistency.
3. Preserve include/exclude, AND within a selection and OR across selections, imported-set intersections, active/inactive handling, relationship properties and explicit unsupported operators. Read unchanged `k6/VC`, `k6/EX` scripts and pools in the pinned checkout before selecting the exact required subset. Roles/groups/terms are retained but are not already a complete filter/ECL provider.
4. Add generated-fixture core/HTTP/PostgreSQL tests immediately after the provider implementation: positive/negative/excluded codes, page totals/order, versions/editions, imports/cycles, updates/rollback, overload/cancellation and memory bounds. Re-run the full current suite; do not weaken assertions.
5. If changing RF2 projection semantics, add an explicit projection revision/rebuild workflow before treating matching source/context hashes as an unchanged import. Existing immutable releases must not be silently overwritten.
6. Resolve the translation compatibility discrepancy explicitly: upstream sends a system URI as R4 `target`, while our adapter reserves it for a ValueSet. Do not silently change standard semantics or modify only benchmark inputs to obtain passes.
7. Load the complete licensed clinical releases and remaining packages, then run the unchanged full upstream preflight. Missing archives are not a reason to fabricate pass coverage. LOINC/RxNorm native ingestion and other filters remain separate required gaps.

## Measuring the goal

Use the original 20 tests/weights, 30-second windows at 1/10/50 VU, one global five-second warmup at 10 VU and five seconds between tests. Keep the same complete corpus, client host, server/database resource limits and cache configuration across sequential local participants. Record revisions, source/pool/wrapper hashes, archive hashes and environment.

Use the documented `bench-round0` / `bench-score` workflow after complete coverage; it calls the pinned original scorer. Keep partial correctness audits separate from comparable scores, weighted totals separate from actual RPS, and historical five-server scores separate from a new local cohort. Optimize the largest measured composite deficits and re-run the matched cohort, not only selected winning points.

Load-test only local instances. Do not stress `tx5.health-samurai.io` or any public server. Commit/push only when asked. Keep reports and this handoff current after the next verified increment.
