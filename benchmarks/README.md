# Local terminology benchmarks

The benchmark has two suites:

- **local**: deterministic, synthetic R4 resources for CodeSystem validation, lookup, ValueSet membership and expansion, subsumption, and translation. No clinical terminology downloads are needed.
- **upstream**: the original [HealthSamurai TX Benchmark](https://github.com/HealthSamurai/tx-benchmark), pinned in [upstream.json](upstream.json). Its k6 scripts and pools run unchanged. The local runner omits the mandatory Prometheus/cAdvisor orchestration; results are JSON files.

Neither suite replaces the HL7 Terminology Ecosystem conformance tests. A passing preflight applies to that scenario's assertions, not to the entire FHIR specification. The upstream suite is particularly oriented towards SNOMED, with LOINC and RxNorm scenarios; UCUM and more HL7/LOINC validation cases are planned separately.

The primary performance KPI is first place by the unchanged upstream composite, including direct local Termbox/FHIRsmith comparisons. See [the Round 0 baseline](../docs/HEALTHSAMURAI_BASELINE.md) for verified published scores, current 2/20 preflight coverage, missing clinical ingestion and comparison limits. Synthetic peaks are not clinical composite scores.

## Prerequisites

- Rust and Cargo, Bun, [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/), `just`, Git, and `tar`.
- For the isolated database: Docker Compose. `bench-migrate` uses the repository CLI; a separate `sqlx-cli` is optional for manual migration administration.
- A release build of the server. The `bench-server` recipe sets this up; debug builds are unsuitable for performance comparisons.

## Run against term-squid

From the repository root:

```sh
# Starts a separate PostgreSQL instance on 127.0.0.1:15433, applies migrations,
# builds the release CLI and imports the generated synthetic package.
just bench-setup

# Run in its own terminal. The server uses port 18081 and the benchmark database.
just bench-server

# In another terminal: inspect correctness before applying load.
just bench-preflight

# Short run of CodeSystem validation, including unknown codes.
just bench --tests TS-CS01 --vus 1,10 --duration 5s --warmup 2s

# All local scenarios, at 1, 10, 50 concurrent clients.
just bench
```

The database has its own Compose project and volume. `just bench-stop` stops its containers without deleting data. `BENCH_DATABASE_URL` overrides the database for the setup, seed and server recipes. The runner defaults to `http://127.0.0.1:18081/r4`; use `--url` or `BENCH_URL` for another instance. The actual application base path is `/r4`.

Fixture generation is also available independently:

```sh
just bench-fixtures
DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  bun benchmarks/bench.ts seed
```

Import goes through the existing CLI, since the REST API has no resource write interface. It creates 10,000 synthetic child concepts, a root, 128 target concepts, a 255-member ValueSet after exclusion, and a ConceptMap with 128 mappings. The fixed `http://example.org/term-squid-benchmark/` canonicals are only benchmark data. Re-import of the same version is idempotent. Bump the fixture version when changing its semantics; the CLI intentionally does not replace existing canonicals with the same version.

## Correctness gates and current gaps

Each local scenario has semantic preflight probes. ValueSet validation includes an excluded code and a code outside the set; expansion checks membership, duplicates, total and pagination. During load, every sampled response is checked again. HTTP 200 alone is insufficient.

Scenarios failing preflight remain visible in `preflight.json` and are not load-tested. A partial run records `status: partial` and a passed/selected coverage count. It can still measure the passing scenarios. No passing scenario, a failed load check or an execution error makes the command fail. Use `--strict` to require all selected preflights to pass as well:

```sh
just bench-preflight --strict
```

The November 2 correctness/performance increment passes all seven local preflights with the original assertions. See [the performance report](../docs/PERFORMANCE_REPORT.md) for the measured build, supported compose subset and remaining conformance gaps. Passing this synthetic suite does not establish full R4 conformance.

## Run the upstream scenarios

```sh
just bench-fetch

# Check the full upstream capability matrix against your local server.
just bench-upstream --preflight-only
```

The original pools require matching content. Synthetic local fixtures do not satisfy them. For a minimal upstream search run, import R4 core into the benchmark database:

```sh
cargo run --release --bin term-squid-cli -- \
  --database-url 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  import hl7.fhir.r4.core --version 4.0.1 -y

just bench-upstream --tests FS01 --vus 1,10 --duration 5s \
  --dataset-label 'hl7.fhir.r4.core#4.0.1'
```

LOINC, SNOMED, RxNorm, VSAC and other upstream scenarios need the corresponding versions and packages loaded separately, following the upstream data instructions and content terms. A failed or skipped preflight may mean missing data as well as an unsupported or incorrect operation.

`--dataset-label` is required for upstream load runs. Use it to record exact loaded versions. `--name fhirsmith --url http://127.0.0.1:7003/tx/r4` can target a local FHIRsmith instance. The local synthetic suite can also target FHIRsmith R4 after loading the generated resources through its `folder:` source. Changing the server name does not provision that server or load its content.

Targets are restricted to loopback by default. `--allow-remote` explicitly enables testing your own remote server. Do not apply load to public community servers.

## Results and comparison

All generated files stay under ignored `.benchmarks/`:

```text
.benchmarks/
  upstream/                       # pinned, unmodified upstream checkout
  fixtures/package/               # resources usable by either server
  fixtures/dataset.json           # fixture version and content fingerprint
  results/<run>/<server>/
    manifest.json                 # environment, server version, coverage, run status
    preflight.json                # semantic failures and capability gaps
    benchmark/<test>_vus<N>.json   # req/s, p50/p95/p99, errors and check failures
    logs/                         # k6 output, including failed executions
```

`--server-revision SHA` records the target's source revision, which cannot be inferred from its package version. The manifest separately records the runner repository revision and whether it has uncommitted changes. The synthetic fingerprint describes the generated fixtures, not the server's entire database; use `--dataset-label` to record any additional loaded packages even for local runs. A run ID is unique by default; explicit existing `--run-id` destinations are refused to preserve results.

Local manifests also record `localHarness.files` and `localHarness.sha256`: the SHA-256 of
JSON-encoded file-name/source tuples for `bench.ts`, `local/cases.js` and `local/load.js`,
in that order. This distinguishes dirty runners at the same commit and separates workload
changes from fixture changes. Use the same harness fingerprint for matched local comparisons.

Warmup results are discarded from the reported measurements. Missing or invalid scenarios receive no real speed reading. Compare matching tests, datasets, content versions, response sizes, concurrency, hardware and resource limits. The optional `observe.py` wrapper samples server CPU/RSS. Open-arrival-rate tests remain unimplemented. The original composite scorer is now available through the commands below; its weighted totals are not per-operation RPS.

## Original composite and local graphs

```sh
just bench-reference             # verify archive hash and reproduce all five historical scores
just bench-round0 --preflight-only # require all 20 checks, without load
just bench-ui                    # original pinned site + separate local results page
```

Open **http://127.0.0.1:4100/tx-benchmark/local/**. The page separates the published composite from actual matched local synthetic charts and the upstream coverage audit. It never inserts synthetic timings into clinical test IDs. The generated site stays under `.benchmarks/ui-site`; pinned site dependencies install from its frozen lockfile without lifecycle scripts. Missing historical raw runs are reported as missing on a new machine. `just bench-ui-build` builds the static report separately in `.benchmarks/ui-build`, without replacing the active dev server’s caches.

Upstream defaults match the pinned runner: 30-second windows at 1/10/50 VU, one five-second global warmup at 10 VU and five-second inter-test pauses. Local defaults remain unchanged. `protocol` and `upstreamHarness` record timing and source/pool/wrapper fingerprints; `timingMatchesRound0` verifies timing only, not data or hardware. Negative LK05 also requires a positive LK01 preflight so a missing whole SNOMED system cannot earn a fast negative lookup result.

For a complete local cohort, first provision all participants with the exact corpus listed in `upstream.json`. Copy `round0-dataset.template.json` to `.benchmarks/round0-dataset.json`, record actual source SHA-256 values, and attest loading only after verification. The attestation does not inspect the database automatically. Record equivalent host, CPU/RAM limits, cache mode and server/database deployment in `--environment-label`; use the same exact metadata file for each participant.

```sh
# Repeat sequentially for termbox and fhirsmith, with their own loopback URLs and revisions.
just bench-round0 --name term-squid --url http://127.0.0.1:18081/r4 \
  --run-id round0-local --server-revision '<source revision>' \
  --dataset-label 'full pinned Round 0 corpus' \
  --dataset-manifest .benchmarks/round0-dataset.json \
  --environment-label '<host, resource limits, cache mode and deployment>'
just bench-score round0-local
bun benchmarks/ui.ts prepare
```

`bench-score` requires complete valid upstream 20×3 matrices for distinct term-squid, Termbox and FHIRsmith participants, recorded revisions, matching corpus/environment/harness, complete preflights and zero load errors/check failures. It calls the original scorer and writes `.benchmarks/composites/<run>.json` and `latest.json`, which the UI displays after preparation. Other complete local participants can join the cohort. This full-coverage admission rule is stricter than upstream's partial-score support; the formula/weights are unchanged. Cohort-dependent normalizers mean local three-server and historical five-server scores are not interchangeable. Never fill local participants using historical measurements.

Run `just bench-test` to check the benchmark's semantic assertions and target selection. For commands and options: `bun benchmarks/bench.ts help`.

## Separate profiling

Keep profiles separate from throughput measurements. On macOS, `sample PID 10 1 -file
.benchmarks/profiles/cpu.txt` and `heap PID` inspect the server. SQL plans should include
`ANALYZE, BUFFERS`; a fast index lookup alone does not measure JSON transfer or decoding.

For approximate allocation churn per validated request, build an isolated instrumented copy (Python 3.12+):

```sh
python3 benchmarks/profile_allocations.py --label before --revision HEAD
# Run the printed binary with the benchmark DATABASE_URL, SERVER_HOST=127.0.0.1,
# SERVER_PORT=18082 and RUST_LOG=warn, then:
bun benchmarks/profile-allocations.ts .benchmarks/allocations-before.json

# Omit --revision to profile the current working tree.
python3 benchmarks/profile_allocations.py --label after
```

The probe wraps Rust's system allocator and counts alloc, alloc_zeroed and realloc calls and
requested bytes. It includes runtime/HTTP/SQL allocations; it does not measure retained memory
or allocator size classes. It validates 1,000 changing codes with 20% missing codes after 20
warmup requests. Use the same build, probe and workload for both sides. Instrumentation changes
timing, so never use its throughput as a performance score. `PROFILE_URL` changes the loopback
probe URL. Instrumented sources, binaries and result files remain under ignored `.benchmarks/`. The builder repairs only local workspace package-version metadata if a historical lockfile lags its manifests; registry dependencies stay locked and offline.

To sample process CPU/RSS during a regular run:

```sh
python3 benchmarks/observe.py --pid PID --output .benchmarks/process.json -- \
  bun benchmarks/bench.ts run --tests TS-CS01 --vus 1,10,50 --duration 30s --warmup 5s
```

CPU time is cumulative, RSS is in KiB, and timestamps allow correlation with the k6 artifacts.
The monitor uses `ps`; profile tools and process visibility depend on the local OS permissions.

For a controlled database-pool experiment, keep the dataset, server build and request matrix
fixed and vary `DATABASE_MAX_CONNECTIONS` (default 32, valid range 1–64):

```sh
DATABASE_MAX_CONNECTIONS=32 just bench-server
```

A larger cap permits more database sessions; it does not replace prepared terminology data.
Record the cap and CPU/RSS with each result. Cold compose preparation has separate single-flight admission; warm checks use the
configured database pool. The measured default is now 32; explicitly set `DATABASE_MAX_CONNECTIONS=5` to reproduce historical five-connection runs. Never compare an old five-connection result with a new default-32 run without labeling the capacity change.

## Prepared ValueSet increment

Apply all migrations before starting the server; the prepared-plan adapter requires
`terminology_generation` and its transactional CodeSystem/ValueSet triggers. Changes from
separate CLI import processes invalidate plans on the next operation without a restart.
The adapter retains at most eight plans and a 64 MiB conservative memory charge; oversized
plans are evaluated without retention. See [the prepared-data report](../docs/PREPARED_DATA_REPORT.md)
for the consistency contract and measured limitations.

TS-VC01 samples changing codes across the synthetic system and 20% distinct missing codes.
Both sides of a comparison must use the same runner revision. For the separate allocation
probe, set `PROFILE_OPERATION=membership` to measure ValueSet membership instead of CodeSystem
validation; keep the setting and driver identical before and after.

The next [warm-read increment](../docs/WARM_READ_REPORT.md) removes serialized SQL checks
from cache hits. TS-VC01 now uses a stable five-request mix: 40% included changing codes,
20% excluded, 20% existing outside the set and 20% changing missing codes. Re-run both server
builds with the same harness; do not substitute historical measurements with a different mix.

The [transport increment](../docs/TRANSPORT_PERFORMANCE_REPORT.md) applies OctoFHIR pool/TCP
patterns: hot connection checkouts skip an extra ping and idle sockets are checked after one
second. TCP_NODELAY remains an experiment; SQLx's return-to-pool check remains.
The idle threshold does not cache data generations. Performance reports compare the same
harness with the previous binary and the pinned local FHIRsmith, keeping pool capacity fixed.

Prepared translation requires migration `20261102000003`. Its ConceptMap trigger and schema
guard preserve cross-process update freshness. See [the translation report](../docs/TRANSLATION_PERFORMANCE_REPORT.md).
The translation workload now has 20% changing missing codes and original positive checks
in both directions; compare both binaries with this same harness. `PROFILE_OPERATION=translation`
selects the separate allocation probe. The ConceptMap cache has its own eight-entry/16 MiB
conservative charge; the existing ValueSet budget is unchanged.

## Subsumption workload

TS-SS01 now checks all four outcomes in preflight and load: `subsumes`, `subsumed-by`,
`equivalent` and `not-subsumed`. Each four consecutive requests have one of each outcome;
each outcome rotates across all 10,000 children. Sibling pairs wrap from the last child to
the first, remaining distinct. Responses must have exactly one `outcome` with the expected
`valueCode`; an OperationOutcome or duplicate output cannot pass as a fast result.

The original root/child preflight and synthetic package version/fingerprint are unchanged.
The request mix is different from the historical positive-only TS-SS01 workload. Re-run the
previous server, the atomic-read server and the pinned local FHIRsmith with this same harness
before comparing performance; do not substitute historical throughput. The new four-outcome
preflight passes on the pinned local reference. A failing scenario receives no score.

`bun test benchmarks` exercises the sampler, response checks and source fingerprint without
PostgreSQL or k6. The benchmark-harness CI job runs these tests with Bun 1.4.2; it does not
apply load or establish server conformance.

The [subsumption comparison](../docs/SUBSUMPTION_PERFORMANCE_REPORT.md) preserves the first
atomic query and the selected exact-case index-bound fix, with fresh local FHIRsmith runs.
The [pool-capacity follow-up](../docs/SUBSUMPTION_POOL_REPORT.md) changes only capacity five
versus 32; those historical runs predate the [new default selection](../docs/POOL_DEFAULT_REPORT.md). Both reports use the same four-outcome harness and
keep diagnostic profiles separate from timed results. The pinned R4 core package requires
the CLI's narrow [legacy v2 compatibility adapter](../docs/R4_CORE_COMPATIBILITY.md);
normal operation version-conflict checks remain strict.


## Prepared hierarchy increment

Apply migration **`20261103000000_hierarchy_generation`** before starting the new server.
It tracks concept/closure projection writes and adds the `hierarchies_tracked` guard;
startup does not apply migrations. Keep these triggers enabled and commit related resource,
concept and closure changes in one transaction. A fresh generation check runs on every
warm request; no TTL or stale SQL-error fallback is permitted.

Hierarchy retention has its own eight-entry / 16 MiB conservative budget. Input admission
allows at most 100,000 concepts, 1,000,000 stored relationships and 32 MiB of projected
strings. Over-limit plans retain a generation-scoped SQL admission marker and execute the
existing atomic SQL instead of publishing an incomplete index. Transient preparation,
in-flight readers, database work and queues are outside the retained-byte limit.

See [the hierarchy report](../docs/HIERARCHY_REPORT.md) for matched four-outcome baseline,
prepared and pinned local FHIRsmith results. Both term-squid binaries use explicit pool 32;
this is an engine comparison, not another capacity experiment. The report also separates
an additional 10,001-concept folded fixture from the unchanged synthetic dataset and records
its own content/workload hashes. Never combine its results with the original fixture fingerprint.


## Fresh generation batching

The latest [generation observer](../docs/GENERATION_BATCHING_REPORT.md) shares one SQL
observation among requests detached before its statement starts. Later arrivals use a
later query, never a completed or already-dispatched observation. There is no generation
TTL; post-commit freshness, PostgreSQL hierarchy key normalization and cold repeatable-read
snapshots remain. Apply all existing migrations; this increment adds none.

Each store allows at most 256 pending observations plus one active batch of at most 256.
A full live queue returns HTTP 503 OperationOutcome, and cancelled queued requests release
admission. This bounds only prepared-observation work, not all HTTP requests or expensive SQL.
Database/guard failures return errors instead of warmed plans. SQLx health/recovery remains.

For matched runs keep explicit pool 32 and the same harness. Measure TS-VC01, TS-EX01,
TS-SS01 and TS-CM01 at 50 VU, add 1/10 VU for subsumption, and retain the reverse-order
50-VU repeat. Use 5-second warmup and 30-second windows. The dataset label must include
any additional folded fixture loaded on both servers, not just the original synthetic hash.
Reference reverse translation and folded hierarchy remain semantic failures, not speed scores.


## Prepared CodeSystem reads

The [concept-read increment](../docs/CONCEPT_READS_REPORT.md) adds a projection index shared
by lookup and CodeSystem validation. Warm reads join only post-admission generation batches;
cold metadata/payloads come from one repeatable-read snapshot. Complete/fragment errors,
business versions, instance selection, PostgreSQL case keys and existing result fields remain.
No migration or dependency is added; migration `20261103000000` and its concept triggers/
`hierarchies_tracked` guard are still required.

The concept cache has eight entries / 32 MiB, bringing all four retained-cache budgets to
128 MiB. Preparation admits at most 100,000 concepts / 32 MiB raw input, with 64 KiB per
property document and admitted metadata. Decoded object graphs are charged incrementally.
Over-limit systems retain SQL admission markers and use the original atomic lookup SQL,
not an incomplete index. These limits do not bound HTTP response clones, cold queues or RSS.

Compare unchanged TS-CS01 and TS-LK01 at explicit pool 32, 1/10/50 VU, 5-second warmup and
30-second windows; repeat loaded lookup in reverse server order. Keep all seven local gates,
fixture/harness fingerprints and extra-content labels. See the report for CPU/RSS, low-rate
losses, fallback limitations and the pinned local reference protocol.

## Inline ValueSet follow-up

Request-local expansion and membership are now supported through the existing bounded fresh compose plans. See [the inline report](../docs/INLINE_VALUESET_REPORT.md) for selectors, versions, the 256-KiB input limit and error behavior. The unchanged upstream audit still passes 2/20: inline requests now reach missing/incomplete clinical dependencies, not a mandatory-URL rejection. This does not establish a clinical score.

Canonical UCUM lookup/CodeSystem validation now use the ready `octofhir-ucum` 0.9.1 library (UCUM 2.2), not an imported common-units list. See [the UCUM contract and release checks](../docs/UCUM_PROVIDER_REPORT.md). This adds product capability, not a standalone Round 0 scenario or measured composite gain.

## Native RF2 and another-device continuation

The CLI now provides `migrate` and checksummed native `import-snomed --manifest FILE [--dry-run]`. Apply migration `20261103000001_snomed_rf2` before starting the new backend. See [RF2 input/storage/read limits](../docs/SNOMED_RF2_REPORT.md) and [the clean-machine handoff](../docs/NEXT_SESSION.md). Clinical files are not committed. Native lookup/validation/subsumption do not imply native ValueSet/implicit/filter/ECL support or an improved composite.

## Native SNOMED ValueSet diagnostics

Apply all migrations, including `20261103000002_snomed_children_index`, before starting the updated backend. Native RF2 dependencies use a separate SQL page/membership path; they must never be treated as empty finite JSON systems. See [provider semantics, validation and measurements](../docs/SNOMED_VALUESET_REPORT.md).

The generated 100,005-concept probe lives in a separate local schema, not the Round 0 corpus. Its definition, load driver, SQL plans, binary hashes and timing/resource samples are retained under `.benchmarks/native-valueset-20261103/` on the measured device only. It does not replace complete clinical imports or the unchanged scorer/protocol. The main corpus still passes 7/7 local gates and 2/20 upstream gates. Do not merge generated native projection data into a clinical attestation.

The subsequent [implicit membership report](../docs/IMPLICIT_MEMBERSHIP_REPORT.md) compares changing whole-system / hierarchy membership at 1/10/50 VU and records separate System-allocator profiling. Those raw artifacts and archived sources live under `.benchmarks/implicit-membership-20261103/`. `before` is invalid because of build overlap; the admitted baseline is `before-clean`. The corpus is generated and does not contribute to a clinical composite.

## Native LOINC primary table

Apply migration `20261103000003_loinc_csv`. The CLI now streams locally obtained `Loinc.csv` through checksummed staging and atomic activation. See [the provider report](../docs/LOINC_PROVIDER_REPORT.md) for source scope, properties, validation, limits and measurements.

Copy [the manifest template](loinc-csv.template.json) next to the extracted source under `.benchmarks/`, fill in the actual business version, lowercase SHA-256 and exact data-row count, and attest `completeTable=true` only for the complete main table. The template intentionally fails unchanged. A content hash/count is provenance, not automatic proof of authentic clinical completeness.

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --release --bin term-squid-cli -- \
  --database-url 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  migrate
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --release --bin term-squid-cli -- \
  --database-url 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  import-loinc --manifest .benchmarks/clinical/loinc-2.82/manifest.json --dry-run
# Repeat the same command without --dry-run to activate the validated release.
```

The main table supports numeric primary-term lookup/validation and numeric implicit membership. Parts, answers/lists, Groups and hierarchy are separate files/semantics. Broad `http://loinc.org/vs` expansion fails explicitly; use explicit numeric concepts or the custom `source-table=Loinc.csv` filter to enumerate only primary terms. Raw CSV axis strings use `raw-*` properties, not standard LP-coded axis meanings. Mixed native SNOMED/LOINC sets share one fresh snapshot; arbitrary finite/native mixing remains unsupported.

The separate generated version `99.0` / 100,005-row schema and measurements under `.benchmarks/loinc-provider-20261103/` are diagnostic only. Never add it to the Round 0 clinical attestation or load only sampled pool codes to obtain a pass.
