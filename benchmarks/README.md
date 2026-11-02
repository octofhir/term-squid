# Local terminology benchmarks

The benchmark has two suites:

- **local**: deterministic, synthetic R4 resources for CodeSystem validation, lookup, ValueSet membership and expansion, subsumption, and translation. No clinical terminology downloads are needed.
- **upstream**: the original [HealthSamurai TX Benchmark](https://github.com/HealthSamurai/tx-benchmark), pinned in [upstream.json](upstream.json). Its k6 scripts and pools run unchanged. The local runner omits the mandatory Prometheus/cAdvisor orchestration; results are JSON files.

Neither suite replaces the HL7 Terminology Ecosystem conformance tests. A passing preflight applies to that scenario's assertions, not to the entire FHIR specification. The upstream suite is particularly oriented towards SNOMED, with LOINC and RxNorm scenarios; UCUM and more HL7/LOINC validation cases are planned separately.

## Prerequisites

- Rust and Cargo, Bun, [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/), `just`, Git, and `tar`.
- For the isolated database: Docker Compose and `sqlx-cli` with PostgreSQL support. Install the latter with `cargo install sqlx-cli --no-default-features --features postgres` if necessary.
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

Warmup results are discarded from the reported measurements. Missing or invalid scenarios receive no speed score. Compare matching tests, datasets, content versions, response sizes, concurrency, hardware and resource limits. The optional `observe.py` wrapper samples server CPU/RSS. The k6 runner does not yet offer open-arrival-rate tests or reproduce the upstream composite score. These additions belong to the performance milestone, before publishing comparative claims.

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
fixed and vary `DATABASE_MAX_CONNECTIONS` (default 5, valid range 1–64):

```sh
DATABASE_MAX_CONNECTIONS=32 just bench-server
```

A larger cap permits more database sessions; it does not replace prepared terminology data.
Record the cap and CPU/RSS with each result. Compose preparation retains its independent
two-worker limit. The default remains five to preserve the original baseline configuration.
