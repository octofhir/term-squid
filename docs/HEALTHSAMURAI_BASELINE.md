# HealthSamurai composite baseline

Updated November 3, 2026. The primary performance objective is **first place by the original TX Benchmark composite**, including direct local comparisons with Termbox and FHIRsmith. Synthetic RPS remains a diagnostic, not the product ranking.

## Pinned reference

- Repository: [HealthSamurai/tx-benchmark](https://github.com/HealthSamurai/tx-benchmark).
- Revision: `78bdffa769e21ac5d654f5dad412d1becf9d7e57` (Round 0 publication).
- [Published Round 0](https://healthsamurai.github.io/tx-benchmark/results/round-0/), May 27, 2026.
- Archive: `site/src/data/round-0.json`, SHA-256 `14ce4d93894044e197363fee4a287d23278f92fe5a3d743e9ef6b0fd3672730e`.
- Original `scripts/lib/scoring.ts` and `config/bias.ts` execute directly from the pinned checkout; neither formula nor weights are replaced.

The reference command reproduces all five published scores from 267 actual per-VU readings:

| Server | Published version | Composite / 100 | Weighted total | Real / imputed tests |
|---|---|---:|---:|---:|
| Termbox | v0.10.1-SNAPSHOT | 100.0000 | 122,697.8 | 20 / 0 |
| Hades | v1.4.1540 | 79.1976 | 97,173.7 | 18 / 2 |
| FHIRsmith | 0.8.6 | 31.9647 | 39,220.0 | 20 / 0 |
| Ontoserver | 6.24.2 | 14.1825 | 17,401.7 | 20 / 0 |
| Snowstorm | 10.10.1 | 6.4787 | 7,949.2 | 11 / 9 |
| term-squid | Not measured on the complete corpus | — | — | No comparable clinical timings |

The weighted total is **not** single-operation throughput. Actual Termbox readings at 50 VU include 6,843.7 RPS for LK01, 6,553.9 for LK02, 6,103.5 for LK03, 7,566.0 for LK04, 6,759.7 for LK05, 6,537.4 for VC01, 8,286.5 for SS01 and 9,182.5 for CM02. These are historical measurements, not measurements from this machine.

The scorer selects maximum error-adjusted throughput across 1/10/50 VU for each server/test, normalizes each test using real participants' LK01/test averages, applies fixed biases, sums, and scales the winner to 100. Missing tests receive the original P15 imputation. Imputed values never become real request readings. Scores and normalizers depend on the participant cohort: a new three-server local ranking is not numerically interchangeable with the historical five-server ranking.

## Protocol and data

Upstream defaults now match its runner: 30-second measurement windows at 1, 10 and 50 VU; one global five-second warmup at 10 VU using the original warmup script; five seconds between tests. Scripts and pools are unchanged. Custom timing is recorded but cannot qualify for the matched composite command. Local synthetic defaults remain 10 seconds / three-second per-test warmup.

Round 0 used Apple M3, eight cores / 24 GB, macOS 26.3 and Docker Desktop 4.64 with an eight-CPU / 20-GB VM. Our historical synthetic runs used an Apple M3 Pro. Do not merge their points or claim cross-host speedups.

Required content identities are recorded in `benchmarks/upstream.json`:

- SNOMED International `20260201`, US `20260301`, UK `20260211`.
- LOINC `2.82`; RxNorm March 2026.
- `hl7.fhir.r4.core#4.0.1`, `hl7.terminology#7.0.1`, `hl7.fhir.us.core#6.1.0`, `us.nlm.vsac#0.24.0`, `hl7.fhir.uv.ips#2.0.0`, `us.cdc.phinvads#0.12.0`, `hl7.fhir.uv.ips#1.1.0`.

Follow the pinned upstream `site/src/pages/data.mdx` for download and content terms. Clinical archives and credentials stay outside Git. The existing database contains synthetic fixtures, R4 core and the extra folded-hierarchy fixture, not this complete corpus. Native RF2/LOINC CSV/RxNorm RRF import is not implemented by our package importer; receiving the archives alone does not finish ingestion.

Copy `benchmarks/round0-dataset.template.json` under `.benchmarks/`, fill the actual archive SHA-256 values and set `attestedLoaded` only after loading/verifying those releases on **each** participant. This is an operator attestation, not automatic database inspection. Reuse the exact same manifest file and environment label. The label must record host, server/database CPU/RAM limits, cache mode and deployment configuration; identical CPU names alone do not establish equal resources.

Only one metadata GET was sent to production `tx5.health-samurai.io`. It identifies Termbox `1.0.3-SNAPSHOT+a15e21b`, FHIR 5.0.0, not the published Termbox version. No production load or writes were sent. The existing local FHIRsmith reference is 0.18.2 at `25a41f87711507789b361480ee4efae42df504d4`, also different from the published version. Provision a separate pinned local Termbox for direct comparisons.

## Initial local coverage audit

The committed engine binary (`f78fd24`, SHA-256 `b1eabe489b7a5de0ff69ab233dd1380dd2cf149aa57c3adc81d6ddd93fa3b244`) ran all twenty original preflight probes against local PostgreSQL, pool 32. **Two pass, nine fail, nine skip.** No load measurements were taken. A passing single preflight does not validate every entry in its random pool.

| Tests | Raw upstream result | Observed blocker |
|---|---|---|
| FS01, LK03 | Pass | Search / one package lookup probe; full pool and corpus not established |
| LK01, LK05 | Fail | R4 core SNOMED stub is incomplete; lookup returns HTTP 400, not a resolved clinical code |
| LK02, LK04 | Skip | LOINC / RxNorm content unavailable |
| VC01–VC03, EX01, EX03, EX04, SS01 | Skip | Required clinical/implicit ValueSet content or operations unavailable |
| EX02, EX05–EX08 | Fail | Inline `valueSet` POST input is not consumed; handler requires `url` |
| CM01, CM02 | Fail | Upstream helper sends a system URI as `target`; our R4 adapter treats `target` as a ValueSet constraint and explicitly rejects it |

“Skip” is the upstream runner's classification, not proof that a route is absent. Fix the observed cause and retest the unchanged pool. Translation needs an explicit version/extension compatibility decision: do not silently reinterpret the R4 ValueSet parameter as a CodeSystem just to pass the benchmark.

The complete strict audit exits 1 before any timing with 2/20 coverage. LK05-only also exits 1 with 0/1. A separate regression test proves that a server returning a false-positive negative lookup cannot time LK05 without a successful positive LK01 context. Raw preflight statuses remain untouched.

Artifacts under `.benchmarks/profiles/healthsamurai-basis/` and `.benchmarks/results/healthsamurai-{full,strict,negative}-audit/` are local and ignored. The benchmark tests pass 29 cases; the isolated generated-fixture scoring smoke executes the real scorer without publishing fixture numbers to the dashboard. No Rust implementation changed in this increment.

## Reproduce and inspect

```sh
just bench-reference
just bench-upstream --preflight-only
just bench-round0 --preflight-only
just bench-ui
```

Open **http://127.0.0.1:4100/tx-benchmark/local/**. This is the original pinned Astro/Svelte/ECharts site with a separate local page, generated in `.benchmarks/ui-site`; production builds use separate `.benchmarks/ui-build` caches. It includes the published composite, our coverage audit, and twelve charts from actual matched synthetic baseline/candidate/FHIRsmith runs. Missing raw runs stay missing on a clean machine. Original Round 0 charts remain available through the page link. No synthetic point is relabeled as a clinical test.

Once all three local servers have the complete corpus and pass the full suite, run each sequentially with the same run ID, metadata file and environment label:

```sh
# Repeat for --name termbox and --name fhirsmith with their own loopback URLs/revisions.
just bench-round0 --name term-squid --url http://127.0.0.1:18081/r4 \
  --run-id round0-local --server-revision '<source revision>' \
  --dataset-label 'complete Round 0 corpus; exact releases in manifest' \
  --dataset-manifest .benchmarks/round0-dataset.json \
  --environment-label '<host, resource limits, cache mode, deployment>'
just bench-score round0-local
bun benchmarks/ui.ts prepare
```

`bench-score` requires complete successful 20×3 measurement matrices, matching source/pool/wrapper fingerprints, recorded revisions, corpus identities and environment, plus zero failed checks/errors. It executes the original formula unchanged; full local coverage means zero imputation. Requiring full coverage here is our **comparison admission rule**, not a change to the published formula or a claim that the upstream runner forbids partial scores. Additional locally measured participants are accepted; historical readings are not inserted. A win over three local servers is not proof of first place against every implementation.

Follow-up: [inline ValueSet evaluation is implemented](INLINE_VALUESET_REPORT.md). The new local audit still passes 2/20, with seven failures / eleven skips; inline scenarios now reach missing/incomplete clinical dependencies instead of failing for a missing URL. Next: resolve translation compatibility explicitly; add reproducible full clinical import/version activation and the SNOMED implicit/filter behavior needed by twelve of twenty tests. Then profile these exact pools and optimize the largest composite deficits. Do not increase cache limits blindly: current prepared concepts/hierarchy limits admit at most 100,000 concepts and therefore fall back on larger releases.
