# Pool and TCP tuning from OctoFHIR — November 2, 2026

## Scope and source of the changes

At 10 VU the selected build exceeds the local FHIRsmith on four of six shared synthetic scenarios: CodeSystem validation 1.41×, lookup 1.07×, ValueSet membership 2.49× and expansion 4.95×. Subsumption and translation still lose (0.25× and 0.44× reference throughput). The overall 3× reference and >12,000 simple-read targets are not achieved. Small margins and sequential laptop runs are not proof of stable superiority. Translation regresses against our previous binary in both the main run and repeat; this is an explicit release-acceptance limitation despite gains on priority reads.

This increment examines existing optimizations in the adjacent OctoFHIR `server-rs` workspace and adapts the pool pattern and evaluates TCP tuning to term-squid's standalone backend. It adds no embedding or runtime dependency on OctoFHIR. Earlier reports and artifacts remain preserved; no commit, push or deployment was performed.

The inspected `octofhir-db-postgres/src/pool.rs` disables SQLx's per-checkout ping; `octofhir-server/src/server.rs` enables TCP_NODELAY on accepted HTTP sockets. The inspected server entry point also supports mimalloc, and CRUD paths have raw-JSON optimizations. The inspected HTTP middleware also uses a 1 KiB compression threshold, compared with our current middleware default of 32 bytes. That is another candidate requiring explicit compressed-client profiling; this increment does not attribute the earlier Bun allocation floor to gzip without measurement. Allocator changes, compression changes and raw-resource responses are deferred: this experiment isolates connection/transport behavior without changing response semantics or adding dependencies. The adjacent workspace had uncommitted changes; evidence snapshots and its HEAD are retained under `.benchmarks/profiles/increment4/`, rather than claiming every inspected line belongs to a committed revision.

## Implementation and failure behavior

Backend pool creation now uses one shared `database::pool_options` helper. It disables unconditional checkout testing and pings only connections idle for at least one second. Hot requests avoid that checkout protocol exchange. The one-second threshold governs socket health checks, not terminology freshness: every warm ValueSet operation still performs the generation SELECT before looking up its immutable plan. CLI commits therefore invalidate positive and negative results with the same snapshot boundary as before.

The installed SQLx 0.8.6 source also pings unconditionally when returning a connection to the pool. That check remains. This change does not eliminate all health traffic or all SQL, and does not claim a database-free hot path. A dead idle socket is rejected by the before-acquire callback; SQLx acquires another connection. A socket that dies inside the one-second health interval can still cause an explicit request error before replacement. The adapter never falls back to stale terminology data on that error. As with any network connection, the database can also fail between a successful ping and the actual query.

TCP_NODELAY was implemented in isolated comparison variants following OctoFHIR's pattern, but was not retained in the production source: the short factor experiment did not establish an independent benefit and combined CodeSystem throughput was lower than with the pool change alone. The final change preserves the previous HTTP listener and all response semantics. Database pool capacity remains five for every primary run; no connection-count increase is included in the reported change.

## Validation and separate diagnostics

Formatting, workspace Clippy with warnings denied, all 22 Rust tests including both PostgreSQL integrations, and eight benchmark harness tests pass. The HTTP/database integration fixture now uses the same pool helper as production. It deliberately terminates a one-connection pool's PostgreSQL backend after an atomic terminology update: after sufficient idle time the first request must recover successfully and return the new resource and dependency marker. For a recent socket, either immediate recovery or an explicit FHIR OperationOutcome is allowed; a subsequent request must recover and see the committed marker. Old cached expansions are never accepted on failure. Existing generation ordering, concurrent readers/writers, version, import, rollback, eviction, negative membership and single-flight assertions are retained.

The first short factor series overlapped the end of variant compilation and is diagnostic only. The clean series repeats all four variants after every build completed, using the same pool five, 2-second warmup and 10-second runs at 10 VU on asserted CodeSystem validation and ValueSet membership. It is a short sequential experiment, not a confidence interval.

| Variant | CodeSystem RPS | CodeSystem p95 ms | Membership RPS | Membership p95 ms |
|---|---:|---:|---:|---:|
| baseline | 3767.3 | 3.817 | 3919.8 | 3.926 |
| tcp-only | 3690.5 | 3.899 | 4050.8 | 3.497 |
| pool-only | 5872.3 | 2.495 | 5335.3 | 2.866 |
| both | 4671.8 | 3.352 | 5455.5 | 2.710 |

The pool-only variant improves both measured workloads; the short TCP-only differences do not establish an independent benefit. The combined variant is slower than pool-only on CodeSystem validation. Accordingly only the idle-aware pool policy is selected for the final primary comparison. TCP variants remain ignored artifacts, not shipped changes.

A separate active CPU sample of the selected release is preserved in `profiles/increment4/after-cpu.txt`; its 12-second traffic driver is diagnostic only and is excluded from the sustained scores. SQLx pool/protocol and HTTP scheduling remain visible. No allocation-count reduction is claimed for this increment: the change removes a hot checkout exchange, while return-to-pool ping and the request allocation floor remain.

## Matched workloads and direct reference

The objective is consistently outperforming FHIRsmith with preserved semantics. The primary comparison uses all six shared passing synthetic scenarios: CodeSystem validation, lookup, ValueSet membership, expansion, subsumption and forward translation. Reverse translation is checked separately but never scored when the reference fails its unchanged assertion. Synthetic results are not full clinical/FHIR conformance.

Each primary scenario uses 5-second warmup and a 30-second sustained run at 10 VU, the same loopback client/harness, Apple M1 host, retained PostgreSQL 17 dataset, synthetic package 1.0.0 and R4 core 4.0.1. Membership maintains 40% changing included codes, 20% excluded, 20% existing outside the set and 20% changing missing codes. Expansions keep the original first/middle/last-page checks. A separate 50-VU membership run examines the previous instability. Server processes run sequentially, with builds and intrusive diagnostics completed beforehand; server CPU/RSS use the same one-second sampler.

| Operation | Before RPS | After RPS | Ratio | Before p50/p95/p99 ms | After p50/p95/p99 ms |
|---|---:|---:|---:|---|---|
| TS-CS01 | 4554.9 | 6158.2 | 1.35× | 1.935/2.960/5.942 | 1.404/2.102/5.139 |
| TS-LK01 | 4770.3 | 5997.6 | 1.26× | 1.848/2.685/6.453 | 1.402/2.285/5.555 |
| TS-VC01 | 5030.6 | 6332.5 | 1.26× | 1.742/2.641/5.707 | 1.332/2.188/4.517 |
| TS-EX01 | 2753.9 | 3460.4 | 1.26× | 2.581/4.893/6.837 | 1.708/4.350/5.843 |
| TS-SS01 | 1760.5 | 2030.8 | 1.15× | 5.364/7.125/10.133 | 4.444/6.798/14.163 |
| TS-CM01 | 3636.2 | 3152.4 | 0.87× | 2.530/3.627/4.374 | 2.684/5.139/8.922 |

All 21 final sustained measurements pass with zero HTTP error rate and zero failed semantic checks. Previous/new term-squid pass all seven preflights; FHIRsmith passes six and its reverse-translation mismatch remains unscored.

| Operation | Before CPU % | After CPU % | Before peak RSS MiB | After peak RSS MiB |
|---|---:|---:|---:|---:|
| TS-CS01 | 56.1 | 61.0 | 41.8 | 42.1 |
| TS-LK01 | 56.8 | 58.8 | 41.8 | 42.1 |
| TS-VC01 | 59.4 | 58.0 | 41.9 | 42.1 |
| TS-EX01 | 65.0 | 65.5 | 42.3 | 16.8 |
| TS-SS01 | 51.9 | 42.8 | 42.3 | 15.7 |
| TS-CM01 | 93.5 | 78.0 | 42.8 | 16.5 |

CPU is the interpolated server-process CPU-time delta (100% means one core); RSS is the maximum one-second sample during that measurement. Database and load-client CPU/RSS are not included.

| Operation | term-squid RPS | FHIRsmith RPS | term-squid / reference | term-squid p50/p95/p99 ms | Reference p50/p95/p99 ms |
|---|---:|---:|---:|---|---|
| TS-CS01 | 6158.2 | 4362.6 | 1.41× | 1.404/2.102/5.139 | 2.002/3.803/5.985 |
| TS-LK01 | 5997.6 | 5628.8 | 1.07× | 1.402/2.285/5.555 | 1.234/3.908/5.880 |
| TS-VC01 | 6332.5 | 2539.3 | 2.49× | 1.332/2.188/4.517 | 2.600/7.709/28.028 |
| TS-EX01 | 3460.4 | 699.5 | 4.95× | 1.708/4.350/5.843 | 13.258/20.586/26.951 |
| TS-SS01 | 2030.8 | 8139.6 | 0.25× | 4.444/6.798/14.163 | 0.991/2.325/3.761 |
| TS-CM01 | 3152.4 | 7128.1 | 0.44× | 2.684/5.139/8.922 | 1.255/2.038/3.014 |

These ratios cover the common synthetic assertions. Reference responses can include richer metadata; they do not establish identical complete FHIR contracts. No composite average is calculated across unlike operations.

| Operation | Reference CPU % | Reference peak RSS MiB |
|---|---:|---:|
| TS-CS01 | 103.0 | 584.9 |
| TS-LK01 | 95.7 | 565.7 |
| TS-VC01 | 102.4 | 554.5 |
| TS-EX01 | 98.9 | 584.2 |
| TS-SS01 | 97.3 | 584.8 |
| TS-CM01 | 103.8 | 584.9 |

The extra 50-VU membership check uses the same 5-second warmup and 30-second duration:

| Server | RPS | p50/p95/p99 ms | CPU % | Sampled peak RSS MiB |
|---|---:|---|---:|---:|
| before | 4976.8 | 9.386/13.743/20.548 | 62.1 | 46.8 |
| after | 4969.6 | 8.402/17.947/30.367 | 45.3 | 24.4 |
| fhirsmith | 3573.9 | 12.627/23.369/29.198 | 120.0 | 602.7 |

At 50 VU the new/reference membership throughput ratio is 1.39×, but throughput is essentially unchanged from the previous build and p95/p99 are higher. The new p99 also slightly exceeds the reference. Translation at 10 VU is 13% slower than the previous build, with worse tails. These losing results are retained.

Because the primary matrix showed a translation regression and worse 50-VU membership tails, both cases were repeated on freshly started previous/new processes with the same pool five, warmup and duration. The repeats are additional evidence and do not replace the primary matrix.

| Operation | Build | RPS | p50/p95/p99 ms | CPU % | Peak sampled RSS MiB |
|---|---|---:|---|---:|---:|
| TS-CM01 (10 VU) | before | 3830.1 | 2.416/3.348/4.170 | 91.1 | 14.9 |
| TS-CM01 (10 VU) | after | 2635.6 | 2.506/8.055/13.881 | 74.0 | 14.9 |
| TS-VC01 (50 VU) | before | 5206.1 | 8.943/13.016/19.637 | 61.6 | 45.8 |
| TS-VC01 (50 VU) | after | 6212.7 | 7.228/12.153/20.155 | 56.4 | 44.5 |

The translation throughput regression reproduces (approximately 31% in this repeat, versus 13% in the primary run). Its cause is not isolated by the pool experiment: repeated ConceptMap decoding/evaluation and runtime scheduling need dedicated profiling before release acceptance. The 50-VU membership repeat improves throughput and p50/p95, but p99 remains slightly worse. Thus the primary p95 regression is not consistently reproduced; the p99 concern remains. All four repeats have zero HTTP/semantic failures. The overall reference objective remains unmet, and these findings must not be replaced by a single best peak.

## Limitations and next work

The largest remaining reference gap is subsumption: the adapter currently resolves two concepts and then queries closure relationships through separate SQL operations. Consolidating this into one atomic read or a reusable immutable hierarchy snapshot is the next engine priority; translation also needs profiling and preparation reuse. The pool experiment does not isolate or resolve those algorithm/storage costs.

A warm ValueSet request still consumes a database generation query and SQLx's return-to-pool health exchange. Larger/shared dependency snapshots, selective invalidation and bounded request admission remain open. The retained-cache charge does not bound total process RSS or queues. Cold preparation still serializes and enumerates complete finite systems. Conformance, UCUM/LOINC and richer metadata gaps from prior reports remain.

These sequential laptop runs do not isolate every host/VM scheduling or networking effect, and short factor probes are not statistical estimates. Throughput and all latency percentiles are reported together; a winning peak does not erase losing scenarios or unstable tails. CPU/RSS cover the measured server process; term-squid's PostgreSQL process is excluded, so server RSS is not a total product memory comparison.

## Reproduction and provenance

Raw profiles, source snapshots, factor binaries, process series, results and logs remain ignored under `.benchmarks/`. See `profiles/increment4/` and `results/increment4-*`; earlier increment directories are untouched. The preserved before binary is `backend-increment4-baseline`, SHA-256 `89fbdc1b49f21f24cabd4e8c5d6180afb92812e76e67bf1e64426f7ac7561a26`. Both term-squid sources are uncommitted descendants of `3b5bb4ff67f038320d8bed551eeaf73239501169`. The selected after binary SHA-256 is `3acb7180c7c92295291faa543a6ff8b300f91308a794a255d9f7705e6e192ce1`; its source archive SHA-256 is `0816d8534136aeee704c077454a904c05b42cd106bf7019ac2dd87f9ebdd00a1`. Exact factor/before/source hashes are recorded in `profiles/increment4/sha256.txt`; the discarded combined source is preserved separately as `both-source.tar.gz`. The reference is based on `25a41f87711507789b361480ee4efae42df504d4`, Node 24.16.0, using its retained local folder configuration and default caching. Two clean-checkout startups failed before measurement because the external package registry returned HTTP 502 while resolving the core version, despite cached content. The measured reference therefore runs an isolated Git-archive copy with one bootstrap-only edit: the R4 core package identifier is pinned from `hl7.fhir.r4.core` to `hl7.fhir.r4.core#4.0.1`. The original checkout stays clean, request/worker code is unchanged, and the cached content version matches prior runs. The exact patch and hashes are retained as `profiles/increment4/fhirsmith-bootstrap-pin.patch` and `reference-bootstrap-sha256.txt`. Only loopback instances receive load.

Use `just bench-db`, apply migrations with `just bench-migrate`, build the selected source and start `just bench-server`. Then run the same current harness for each binary and the local reference:

```sh
bun benchmarks/bench.ts run --tests TS-CS01,TS-LK01,TS-VC01,TS-EX01,TS-SS01,TS-CM01 \
  --vus 10 --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1'
```

Use `--name fhirsmith --url http://127.0.0.1:17003/tx/r4` and the pinned reference revision for that local server. Wrap runs with `benchmarks/observe.py` for CPU/RSS, as documented in the benchmark README.
