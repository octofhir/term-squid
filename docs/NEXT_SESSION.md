# Resume development on another device

The user-selected R4 API implementation now covers all 70 declared inputs of six operations, bounded supplements, public read workflows and standalone administration. Read [API_IMPLEMENTATION_REPORT.md](API_IMPLEMENTATION_REPORT.md), [API_REQUIREMENTS.md](API_REQUIREMENTS.md) and [R4_API_CONTRACT.json](R4_API_CONTRACT.json). The full Rust/PostgreSQL checkpoint passes 147 tests; 23 embedded production browser/API scenarios, 29 harness tests and seven unchanged local gates also pass; exact delivery evidence is recorded in the report. Apply migration `20261104000006_supplements` and build frontend before RustEmbed. Preserve provider/registry bounds and do not silently switch to performance work without the next user-selected priority.

The branded UI/admin and local-package increments remain implemented; see [the UI report](UI_ADMIN_IMPLEMENTATION_REPORT.md), [the package report](PACKAGE_IMPORT_REPORT.md) and [NEXT_SESSION_UI.md](NEXT_SESSION_UI.md) when changing those areas. Keep the UI live on 5173 when available. The engine/composite continuation below is a separate workstream; preserve its existing changes and measurements.

The delivery is preserved in `d4bb6c1`, following native-provider checkpoint `b426651`, and integrated with the independent `7cef264` dependency release. The combined workspace version is **0.1.3**, with SQLx 0.9, SHA-2 0.11 and UCUM 0.9.3. Read the post-merge validation in the API report; retain both histories and the independent-core CI guard. The user authorized this commit/push; future commits, pushes, publication and deployment still require their own explicit request.

## Goal and current state

The primary KPI is **first place by the unchanged HealthSamurai TX Benchmark composite**, with direct local Termbox/FHIRsmith comparisons. Do not replace it with synthetic peaks, changed weights, sampled-only clinical imports or historical different-host readings.

This repository snapshot includes code, benchmark integration, reports and plans. Before switching devices, inspect the working tree and make sure the intended changes are available in the remote or another explicit transfer. A local commit alone cannot be fetched from another computer; committing and pushing require the user's instruction.

Read in order:

1. `AGENTS.md` and this file.
2. `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`.
3. `docs/HEALTHSAMURAI_BASELINE.md`, `benchmarks/README.md`.
4. `docs/LOINC_PROVIDER_REPORT.md`, `docs/IMPLICIT_MEMBERSHIP_REPORT.md`, `docs/SNOMED_VALUESET_REPORT.md`, `docs/SNOMED_RF2_REPORT.md`, `docs/INLINE_VALUESET_REPORT.md`, `docs/UCUM_PROVIDER_REPORT.md`.
5. `docs/HTS_FEATURE_GAPS.md` and the relevant performance report before changing the measured engine.

Implemented:

- Branded React 19 public browsing and separate authenticated administration, fixed Git UI-kit delivery, shared transactional JSON/package ingestion and protected persistent import jobs. See [the package report](PACKAGE_IMPORT_REPORT.md); apply migrations through `20261104000005_admin_package`; build frontend before RustEmbed. See the UI report for bootstrap/configuration, exact support limits and validation. No default password or OctoFHIR backend/database dependency.

- Prepared lookup/validation, finite ValueSets, ConceptMaps and hierarchies with bounded retention and fresh cross-process generation observations. No TTL/stale-error fallback; pool default 32.
- Inline ValueSet expansion/membership through the finite engine, content-fingerprinted request-local keys, no persistence and explicit unsupported input errors.
- Ready `octofhir-ucum = 0.9.1`, model 2.2: canonical grammar lookup/CodeSystem validation without SQL and Rust comparability. UCUM ValueSet integration remains incomplete.
- Native checksummed RF2 Snapshot import, atomic activation, selected-dialect display projection and bounded active inferred subsumption. A bounded native ValueSet provider now handles implicit whole/hierarchy sets, inline/stored/instance compose, hierarchy and selected property filters. ECL/refsets and mixed native/finite composition remain explicit errors; see the new report.
- Typed implicit SNOMED whole/isa membership now resolves release, anchor and member in one data statement inside the existing transaction. Allocation churn drops 31%/41%; no cache/TTL, pool or timeout change. See `docs/IMPLICIT_MEMBERSHIP_REPORT.md` for timing variability and retained baseline/candidate evidence. That checkpoint passes 81 Rust tests and 29 harness tests.
- Native LOINC primary-table import streams checksummed `Loinc.csv`, preserves source fields and atomically activates immutable releases. Numeric reads, implicit numeric membership, explicitly scoped primary filters and mixed native SNOMED/LOINC sets are implemented. Accessory codes and all-code expansion remain explicit errors. Typed `$lookup` returns seven R4 property types. Latest checkpoint: 101 Rust tests and 29 harness tests; see `docs/LOINC_PROVIDER_REPORT.md`.
- Original Round 0 scorer/pools/protocol and a local graph site. The five historical scores reproduce; no comparable term-squid clinical composite is claimed. Existing-corpus preflight is still 2/20, not an ongoing crashed load test.

Plans and trackers live under `vibe/healthsamurai-*`, `vibe/inline-valueset-*`, `vibe/ucum-provider-*`, `vibe/snomed-rf2-*`, `vibe/snomed-valueset-*`, `vibe/implicit-membership-*` and `vibe/loinc-provider-*`. A completed tracker covers that increment only, not completion of the composite/product goal.

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

Apply **all** migrations, including `20261103000001_snomed_rf2`, `20261103000002_snomed_children_index` and `20261103000003_loinc_csv`, before starting the updated backend. Startup does not apply them. Do not load native releases while running an older backend. Never copy one compiled binary across OS/architectures; rebuild locally. Stop a running binary before replacing its file, or use a fresh immutable artifact filename.

## What Git does not contain

`.benchmarks/` is ignored: no database volumes, clinical archives, extracted files, secrets, generated fixtures, raw timing/profile JSON, binaries, upstream/FHIRsmith/HTS checkouts, Node/k6/browser installs or generated site dependencies. Historical reports contain summaries and fingerprints, not portable copies of those artifacts.

Recreate the synthetic database with the recipes. The two previously passing upstream probes also require R4 core 4.0.1; synthetic-only setup does not reproduce that coverage. If its registry download is unavailable, import an independently obtained official local `.tgz` archive rather than substituting a different version. Obtain clinical files locally under their content terms. RF2 importer source-file hashes and the benchmark corpus/archive manifest are different provenance layers; record both correctly. Never mark clinical corpus attestation complete merely because synthetic setup passed.

For acquisition links, authentication requirements and the upstream Gallery/FHIRsmith cache-version discrepancy, read `docs/CLINICAL_DATA_SOURCES.md`. For the clinical corpus, use the exact versions in `benchmarks/upstream.json`. Start the SNOMED import from `benchmarks/snomed-rf2.template.json` and the Round 0 corpus attestation from `benchmarks/round0-dataset.template.json`. Both templates intentionally fail unchanged. Native loader accepts complete merged Snapshots, not standalone extension/Full/Delta files. Native LOINC main-table import now uses `benchmarks/loinc-csv.template.json`, which intentionally fails unchanged. It does not include Parts/answers/hierarchy or establish full all-code completeness. Native RxNorm import is not implemented yet.

The prior pinned local FHIRsmith setup is documented in the performance reports: source `25a41f87711507789b361480ee4efae42df504d4`, version 0.18.2, with recorded bootstrap-only package/loopback changes. Do not substitute its results for published FHIRsmith 0.8.6. Local Termbox must be provisioned separately and pinned by image digest/revision. The production tx5 metadata identified a different version and was not load-tested.

## Next work: clinical coverage and native execution cost

1. Read `docs/LOINC_PROVIDER_REPORT.md`, then `docs/IMPLICIT_MEMBERSHIP_REPORT.md` and `docs/SNOMED_VALUESET_REPORT.md`. Start at `crates/backend/src/store/implicit_membership.rs` / `native_valueset.rs`. Latest validation is 101 Rust tests and 29 harness tests. Native LOINC starts at `crates/cli/src/loinc.rs`, `crates/core/src/loinc.rs` and the shared native SQL provider; do not repeat the implemented primary-table loader or typed lookup increment. Native expansion improves about 1.7x on the generated 100k probe; membership p99 at 50 VU remains variable. Implicit whole/isa membership has a typed descriptor and one data statement; general native compose still recompiles bounded metadata and executes recursive SQL per operation. It does not retain a native graph/plan cache. `compose_snapshot` still sends native dependencies to this separate page/membership provider; do not remove that dispatch guard or feed empty RF2 JSON into the finite engine.
2. Profile the exact unchanged clinical pools after loading the full releases. Generated 100k-concept measurements are diagnostics, not clinical coverage or a composite. Large hierarchy expansion still visits descendants in SQL, orders matched projections and searches text; count and page now share one stream and broad/small branches use actual bounded cardinality. Native transaction round trips and remaining allocation sizes still limit membership. The SNOMED implicit single data statement is already implemented; do not repeat that increment. The LOINC 100,005-row CodeSystem fixture exceeds the existing 100,000-concept prepared-payload admission and uses SQL fallback; allocation churn stays near 358 KiB/request. Its primary 50-VU loss does not reproduce in the restart throughput repeat, while p99 losses remain; preserve all series. Complete native LOINC membership timing on a quiet host, because external builds interrupted its attempts. Profile allocation sizes/stacks before attributing them to future frames or the driver; investigate bounded compact validation indexes/native plans with fresh cross-process generation checks. Do not raise timeouts, traversal/cache limits or change pool weights to hide failures.
3. Preserve one-operation snapshot consistency, latest/explicit version and instance selection, the International implicit default, include/exclude, imported intersections, inactive handling, deterministic pages and explicit unsupported operations. Common ValueSet admission is 64 total requests, with four executing native application futures, a 15-second native-phase deadline, ten-second SQL statements and a conservative page budget checked before JSON serialization. Keep cancellation, rollback, stale-positive/negative and overload tests. SQLx may drain canceled statements until the server timeout; preserve the pool bound and improve database-side cancellation/work accounting rather than claiming immediate SQL cancellation.
4. Complete mixed native/finite composition, selected-dialect synonym search, supported remaining properties and required ECL/refset semantics. Unsupported cases must continue failing explicitly until implemented. LOINC accessory Parts/links, answers/lists, Groups and hierarchy remain; raw-* axes are custom strings, never LP identifiers. Load the actual complete LOINC 2.82 main table for LK02 when available; EX07 still needs broader content/RxNorm. Native RxNorm ingestion remains required for the pinned corpus. Never import only sampled pool codes or relabel generated fixtures as a clinical release.
5. RF2 projection semantics have not changed; the RF2 reverse-edge migration `20261103000002` only adds an inferred-edge index; `20261103000003` separately adds LOINC tables. Future projection changes still require an explicit revision/rebuild workflow; immutable source/context fingerprints must not silently preserve changed projections.
6. Resolve the translation compatibility discrepancy explicitly: upstream sends a system URI as R4 `target`, while our adapter reserves it for a ValueSet. Do not silently change standard semantics or modify only benchmark inputs to obtain passes.
7. Load the complete licensed releases and remaining packages, then re-run all twenty original preflights before claiming a comparable composite. Main benchmark content still has no native clinical releases. The separate generated diagnostic schema and all raw artifacts are gitignored and do not travel to another laptop.

## Measuring the goal

Use the original 20 tests/weights, 30-second windows at 1/10/50 VU, one global five-second warmup at 10 VU and five seconds between tests. Keep the same complete corpus, client host, server/database resource limits and cache configuration across sequential local participants. Record revisions, source/pool/wrapper hashes, archive hashes and environment.

Use the documented `bench-round0` / `bench-score` workflow after complete coverage; it calls the pinned original scorer. Keep partial correctness audits separate from comparable scores, weighted totals separate from actual RPS, and historical five-server scores separate from a new local cohort. Optimize the largest measured composite deficits and re-run the matched cohort, not only selected winning points.

Load-test only local instances. Do not stress `tx5.health-samurai.io` or any public server. Commit/push only when asked. Keep reports and this handoff current after the next verified increment.
