# Subsumption performance — November 3, 2026

> Historical configuration: the measurements below use explicitly recorded capacities. The server now defaults to 32 after the [six-workload default selection](POOL_DEFAULT_REPORT.md). Set `DATABASE_MAX_CONNECTIONS=5` when reproducing a five-connection series.

## Outcome

At 10 VU the selected build reaches **2,922 RPS versus 1,107 for the previous build (2.64x)**, with p99 falling from 19.767 to 7.357 ms. Its fresh repeat reaches 3,209 RPS. The pinned local FHIRsmith reaches 10,239 RPS in the final comparison: selected term-squid is only 0.285x reference throughput. The overall 3x reference and >12,000 simple-read RPS targets remain unachieved.

All 26 sustained measurements across the intermediate and selected series pass: **3,099,603 response checks, zero HTTP errors and zero failed checks**. This is a local synthetic subsumption result, not complete FHIR conformance or an overall terminology score.

## Workload and deployment

The unchanged synthetic package 1.0.0 has 10,000 children under one root. TS-SS01 now has a fixed 25% mix of `subsumes`, `subsumed-by`, `equivalent` and `not-subsumed`; each outcome rotates through the whole child pool. All servers use the same harness, including its stricter single-outcome checker. Historical positive-only TS-SS01 numbers are not used as baselines.

Both deployments load the same synthetic package and R4 core 4.0.1. The CLI needed a narrow [legacy R4 core import adapter](R4_CORE_COMPATIBILITY.md); the first import attempt rolled back, and the successful retry imported all 2,458 terminology resources. The archive stays unchanged. FHIRsmith already normalizes these legacy canonicals at load time. Both Rust binaries read the same populated database: 1,064 CodeSystems, 1,317 ValueSets and 81 ConceptMaps.

- Host: Apple M3 Pro, 12 cores, 36 GiB RAM, macOS 26.7.1. This is not the M1 machine in historical reports.
- PostgreSQL 17.10 runs in the existing Docker/Colima VM with 2 vCPUs and 4,095,365,120 bytes of reported memory. The VM also has other local services. No dedicated host reservation or per-process resource caps were applied.
- Rust 1.99.1 release binaries use pool capacity five and the existing idle-aware health policy. The return-to-pool SQLx health exchange remains; there is no generation TTL or TCP_NODELAY source change.
- FHIRsmith 0.18.2 / txVersion 1.10.1 uses Node 24.16.0, pinned source `25a41f87711507789b361480ee4efae42df504d4`, default terminology/expansion cache settings and only the TX R4 endpoint enabled. npm audit and daily reports are disabled. Its recorded bootstrap patch pins core to `hl7.fhir.r4.core#4.0.1` and binds the listener to loopback; query/worker code is unchanged.
- Bun 1.4.4 and k6 2.3.0 use loopback, 5-second discarded warmup at 1 VU and 30-second measured windows at each concurrency. Servers run sequentially as fresh processes. No build or intrusive profile overlaps timing.

This compares those complete local deployments, including PostgreSQL VM/transport costs versus native Node/SQLite; it does not isolate language or library-engine throughput. CPU is interpolated from cumulative server-process CPU time (100% = one core). RSS is the maximum one-second `ps` sample in each window. PostgreSQL, VM and load-client CPU/RSS are excluded from the Rust server figures; those figures do not establish lower total product memory use.

Both Rust builds pass all seven full-suite preflights. FHIRsmith passes six; its known reverse-translation response still returns target T00007 instead of source C00007. That failure is retained and receives no speed score. All three servers pass the four-outcome subsumption gate before load.

## Why the first atomic version was not enough

Consolidating two concept reads and up to two closure reads into one statement preserves a single operation snapshot. However, the first compact query used an OR across case policies. With R4 core loaded, PostgreSQL chose an index-only scan bounded by system ID, filtering up to 10,000 concepts before finding `root`. Merely seeing an index scan in EXPLAIN was not sufficient.

The selected query uses separate, mutually exclusive `UNION ALL` branches for exact and case-folded code resolution. Exact-case code equality is now an index condition. This keeps the single-statement snapshot, stored spelling, explicit version selection and errors unchanged. Closure lookup uses the composite key and skips unnecessary branches. All 29 Rust tests, including version/case/update database checks, Clippy with warnings denied and twelve harness tests pass.

The diagnostic generic plan for the case-insensitive branch can still choose a system-range scan rather than the expression index. That branch is correct in regression tests but remains a performance limitation; the current synthetic load is case-sensitive. No broad case-insensitive speedup is claimed.

## Selected final comparison

Fresh previous, indexed and reference processes run in that order:

| Build | VU | RPS | p50 / p95 / p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---:|---|---:|---:|
| Previous | 1 | 578.9 | 1.691 / 2.273 / 3.685 | 18.5 | 11.2 |
| Previous | 10 | 1,106.6 | 8.852 / 14.384 / 19.767 | 47.7 | 13.3 |
| Previous | 50 | 1,105.3 | 45.822 / 69.977 / 91.955 | 47.7 | 22.2 |
| Selected indexed | 1 | 1,470.9 | 0.559 / 0.876 / 1.390 | 20.0 | 11.3 |
| Selected indexed | 10 | 2,922.4 | 3.103 / 4.930 / 7.357 | 56.1 | 13.4 |
| Selected indexed | 50 | 3,288.1 | 13.920 / 21.295 / 29.958 | 55.8 | 22.2 |
| FHIRsmith | 1 | 4,285.6 | 0.170 / 0.216 / 0.640 | 54.4 | 410.3 |
| FHIRsmith | 10 | 10,238.6 | 0.831 / 1.315 / 3.139 | 104.7 | 322.7 |
| FHIRsmith | 50 | 10,038.3 | 4.624 / 7.948 / 11.184 | 104.8 | 361.1 |

Selected/previous throughput is 2.54x, 2.64x and 2.97x at 1/10/50 VU. Selected/reference throughput is 0.343x, 0.285x and 0.328x. Every selected latency percentile improves against the paired previous build, but every selected throughput and p99 still loses to the reference.

The selected build is repeated as another fresh process with the same protocol:

| Build | VU | RPS | p50 / p95 / p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---:|---|---:|---:|
| Selected indexed | 10 | 3,209.1 | 2.775 / 4.616 / 7.402 | 55.2 | 13.2 |
| Selected indexed | 50 | 3,274.4 | 14.247 / 21.281 / 27.890 | 56.3 | 22.0 |

The repeat does not replace the primary results. In particular, its better 10-VU RPS is not the headline score, and its 10-VU p99 is slightly worse than the primary selected point.

## Preserved intermediate series

Before the index-bound fix, previous, atomic-OR and reference processes ran in that order with the same data and harness:

| Build | VU | RPS | p50 / p95 / p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---:|---|---:|---:|
| Previous | 1 | 566.1 | 1.751 / 2.230 / 3.084 | 18.3 | 11.1 |
| Previous | 10 | 1,322.8 | 7.542 / 11.844 / 15.015 | 47.5 | 13.3 |
| Previous | 50 | 1,394.0 | 38.495 / 51.107 / 66.465 | 47.8 | 22.2 |
| Atomic OR | 1 | 808.2 | 1.203 / 1.527 / 2.010 | 10.6 | 11.0 |
| Atomic OR | 10 | 2,274.4 | 4.057 / 6.618 / 9.698 | 36.1 | 13.0 |
| Atomic OR | 50 | 2,069.4 | 22.280 / 32.431 / 38.724 | 37.1 | 21.9 |
| FHIRsmith | 1 | 4,493.2 | 0.168 / 0.196 / 0.366 | 54.6 | 412.0 |
| FHIRsmith | 10 | 10,574.2 | 0.814 / 1.184 / 2.974 | 106.3 | 411.9 |
| FHIRsmith | 50 | 10,422.2 | 4.483 / 7.567 / 10.545 | 105.0 | 414.4 |

Fresh repeats then reverse server order: reference, atomic-OR, previous:

| Build | VU | RPS | p50 / p95 / p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---:|---|---:|---:|
| Previous | 10 | 1,250.6 | 8.341 / 11.633 / 14.218 | 48.8 | 13.3 |
| Previous | 50 | 1,245.4 | 43.945 / 57.485 / 66.182 | 49.3 | 22.1 |
| Atomic OR | 10 | 2,207.0 | 4.281 / 6.317 / 8.171 | 36.9 | 13.1 |
| Atomic OR | 50 | 2,210.0 | 21.802 / 28.522 / 33.474 | 37.3 | 22.0 |
| FHIRsmith | 10 | 10,613.2 | 0.819 / 1.172 / 2.977 | 106.3 | 413.6 |
| FHIRsmith | 50 | 10,333.3 | 4.525 / 7.676 / 10.761 | 106.0 | 413.8 |

The intermediate primary atomic/previous ratios are 1.43x, 1.72x and 1.48x. The 10/50-VU repeats give 1.76x/1.77x. These smaller improvements and all losing reference points remain visible. Previous-build 10-VU throughput also varies from 1,323 in the first primary series to 1,107 in the final series. These sequential laptop runs are not randomized confidence estimates or a universal fixed speedup factor.

## Evidence and reproduction

All raw material stays ignored under `.benchmarks/`. Result prefixes are `subsumption-primary-*`, `subsumption-repeat-*`, `subsumption-final-*` and `subsumption-final-repeat-indexed`. Eligibility preflights are kept separately. `profiles/subsumption/` contains process samples, both summary JSON files, SQL plans, bootstrap patch, source archives, compiler/test logs and checksums. Setup and orchestration scripts, downloaded Node/k6 and the reference checkout are local artifacts, not prerequisites guaranteed by a clean checkout.

- Previous source: commit `65a210d0c213a66176a57e9297d3fd7f8b21aecc`; binary SHA-256 `aad0c7b2e287fa2ebb9a3a267e2bf5ca42a0bcc80b7077d6bff1b9980c58921d`.
- Intermediate binary SHA-256: `d2e29650a1a7dfac207654e6bfc6586554b9f7cd1366fb66632f03d35ff69d4f`.
- Selected binary SHA-256: `cfafe8f0f8099fe286622a280f7e19180aef164e4d919cc10cb01f2329952a3f`; source archive SHA-256 `dd58572c85023707c79eda53a1619c30a503180165e68d9cecfd35b1d998713b`.
- Shared local harness fingerprint: `732026ed5adb0e59bf9fa50c46568c348ea6d262bff84dd866a34510aaba8399`.
- Synthetic resource fingerprint: `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017`.
- R4 core archive SHA-256: `ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44`.

To recreate the comparison, build the previous revision in an isolated ignored source directory and this working tree in release mode, retaining separate binaries. Start the isolated PostgreSQL database, apply all repository migrations and import synthetic 1.0.0 plus the unchanged R4 core archive through the corrected CLI. On hosts without the Compose plugin use `docker-compose -f benchmarks/compose.yaml up -d --wait`.

For FHIRsmith, check out the pinned revision, use Node 24.16.0 and `npm ci --omit=dev --no-audit --no-fund`, configure only `/tx/r4` with `fhirVersion: "4.0"`, and set `librarySource` to a YAML containing `base.url` and a `sources` entry `folder:/absolute/path/to/.benchmarks/fixtures/package`. Place the extracted core archive under `<FHIRSMITH_DATA_DIR>/terminology-cache/hl7.fhir.r4.core#4.0.1/package`, pin only the R4 startup package identifier in `tx/library.js`, and bind `server.js` to loopback. Preserve that patch and leave query/worker code and default cache settings unchanged.

Require semantic preflight on each server, then use the same current runner for all three, with CPU/RSS observation as described in the benchmark README:

```sh
bun benchmarks/bench.ts run --tests TS-SS01 --vus 1,10,50 \
  --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import)'
```

For the reference add `--name fhirsmith --url http://127.0.0.1:17003/tx/r4` and its pinned `--server-revision`. Keep the reference's failed reverse translation unscored. Repeat on fresh processes and retain all results; never compare a new-machine number directly with historical M1 throughput.

## Next work

The subsequent [pool-capacity follow-up](SUBSUMPTION_POOL_REPORT.md) records a separate CPU profile and ten additional matched measurements. More connections help, but still do not beat the reference; the production default remains five.

The remaining reference gap is substantial. Profile the database/transport/pool path and request scheduling separately from timing before choosing the next optimization; evaluate reusable hierarchy snapshots only with post-commit freshness intact. Add a large case-insensitive workload before changing its plan/index strategy. Closed-concurrency synthetic results do not cover arrival-rate overload, clinical providers, large hierarchies or mixed updates.
