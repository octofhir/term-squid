# Sources for the pinned clinical corpus

Checked November 3, 2026. The pinned HealthSamurai repository documents the sources in [`site/src/pages/data.mdx`](https://github.com/HealthSamurai/tx-benchmark/blob/78bdffa769e21ac5d654f5dad412d1becf9d7e57/site/src/pages/data.mdx). These are acquisition instructions, not evidence that the local database contains the complete releases.

| Required content | Source | Access / local status |
|---|---|---|
| SNOMED International `20260201` | [SNOMED MLDS](https://mlds.ihtsdotools.org/) | Publisher account/content access required; no archive downloaded in this session |
| SNOMED US `20260301` | [NLM archive, March 2026](https://www.nlm.nih.gov/healthit/snomedct/archive.html) | Exact RF2 ZIP link redirects to UMLS login |
| SNOMED UK `20260211` | [NHS TRUD releases](https://isd.digital.nhs.uk/trud/users/authenticated/filters/0/categories/26/items/101/releases) | Redirects to sign-in |
| LOINC `2.82` | [LOINC archive](https://loinc.org/archive) | Archive lists 2.82; [download documentation](https://loinc.org/downloads/) requires an account. Current 2.83 is a different corpus |
| RxNorm `03022026` | [NLM RxNorm files, previous releases](https://www.nlm.nih.gov/research/umls/rxnorm/docs/rxnormfiles.html) | Exact full March ZIP redirects to UMLS login. The public Prescribable subset is not a replacement for the full release |

The exact publisher link for US is `https://download.nlm.nih.gov/mlb/utsauth/USExt/SnomedCT_ManagedServiceUS_PRODUCTION_US1000124_20260301T120000Z.zip`. RxNorm is `https://download.nlm.nih.gov/umls/kss/rxnorm/RxNorm_full_03022026.zip`. They are recorded for reproducibility, not represented as anonymous downloads. LOINC also documents an [authenticated download API](https://loinc.org/kb/api/download).

## What the benchmark configurations actually load

[`servers/termbox/data.yaml`](https://github.com/HealthSamurai/tx-benchmark/blob/78bdffa769e21ac5d654f5dad412d1becf9d7e57/servers/termbox/data.yaml) uses Termbox `gallery` entries for all five clinical editions/releases, plus local `.tgz` FHIR packages. The [Gallery documentation](https://www.health-samurai.io/docs/termbox/loading-data/gallery) describes canonical URL/version selection inside Termbox. The reviewed configuration/docs do not provide independent RF2/CSV/RRF download URLs for that Gallery. A Gallery import in Termbox does not establish that term-squid can decode its prepared binary format or that we have the original source archives.

The pinned [`servers/fhirsmith/README.md`](https://github.com/HealthSamurai/tx-benchmark/blob/78bdffa769e21ac5d654f5dad412d1becf9d7e57/servers/fhirsmith/README.md) describes automatic GCS caches for SNOMED `20250201` and LOINC `2.81`. The same [`library.yml`](https://github.com/HealthSamurai/tx-benchmark/blob/78bdffa769e21ac5d654f5dad412d1becf9d7e57/servers/fhirsmith/library.yml) also names `sct_us_20250901.cache`, `sct_uk_20230412.cache` and `rxnorm_02032025-a.db`. Those differ from the corpus table. Verify the actual loaded release/context of every local participant; do not copy those caches into a `20260201` / `2.82` attestation or infer equal content from the benchmark overview alone.

## Local import boundary

Keep archives, extracted files and source/checksum manifests under `.benchmarks/clinical/`. Populate `benchmarks/snomed-rf2.template.json` from complete merged Snapshots, then use the CLI dry-run/import workflow in [the RF2 report](SNOMED_RF2_REPORT.md). Do not substitute standalone extension/Delta files or benchmark-pool samples. Record SHA-256 for the actual archives and selected source files; templates and documentation do not attest completeness.

The native SNOMED loader exists. Native LOINC main-table import now exists; use `benchmarks/loinc-csv.template.json` and [the provider report](LOINC_PROVIDER_REPORT.md). Accessory Parts/answers/hierarchy and native RxNorm ingestion remain implementation work even after obtaining their files. The seven FHIR package versions are pinned in `benchmarks/upstream.json`; package availability and import success must also be checked separately. No clinical archives were acquired and no complete clinical corpus was attested during this session.
