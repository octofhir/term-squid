# Server pool default: 32 — November 3, 2026

## Decision

The backend now defaults to **32 database connections per process**, replacing five. Explicit `DATABASE_MAX_CONNECTIONS` values remain supported from 1 to 64, including five. The change is in `crates/backend/src/config.rs`, `.env.example` and the environment-variable documentation. CLI import pools are unchanged.

The decision uses six shared warm-read scenarios, not only subsumption. At 50 VU in the matched selection matrix, capacity 32 improves throughput over 16 by **12.0–18.7% on every operation**, with lower p99 in all six. Against capacity five it improves throughput by **1.64–1.85x**, also with lower p99 everywhere. No failed response is counted as fast work.

This is a measured default for the tested standalone deployment, not a universal sizing rule or an achieved overall 3x reference / >12,000 simple-read target. Operators sharing PostgreSQL across replicas must budget their combined sessions, CLI imports and administration, and override the per-process cap when necessary.

## Selection protocol

The selection rule was recorded before timing: reject HTTP/semantic failures, repeat material throughput/p99 regressions before deciding, and prefer 32 only if it beats 16 by at least 10% on at least three of six operations without a confirmed regression elsewhere. Capacity 32 meets that rule on all six, so no regression-driven rerun was required.

The same indexed release binary is used for explicit capacities 5, 16 and 32. Each fresh server runs the six scenarios in the order below, with a 3-second discarded warmup at 1 VU and 15-second windows at 50 VU. A fresh pinned FHIRsmith follows with the same protocol. All 24 selection points pass; raw p50/p95/p99, CPU/RSS and reference results are retained.

| Operation | Pool 5 RPS | Pool 16 RPS | Pool 32 RPS | 32 / 16 | p99: 5 / 16 / 32 ms |
|---|---:|---:|---:|---:|---|
| CodeSystem validation | 3,407.9 | 5,333.0 | 6,113.2 | 1.146x | 25.530 / 18.625 / 18.211 |
| Lookup | 3,405.9 | 5,381.0 | 6,047.6 | 1.124x | 26.446 / 19.829 / 18.048 |
| ValueSet membership | 3,676.9 | 5,855.0 | 6,802.8 | 1.162x | 21.027 / 14.748 / 14.125 |
| Expansion | 2,831.6 | 4,134.9 | 4,646.8 | 1.124x | 25.708 / 17.668 / 17.326 |
| Subsumption | 3,373.3 | 4,903.7 | 5,821.2 | 1.187x | 25.812 / 22.513 / 20.062 |
| Forward translation | 3,638.8 | 5,781.9 | 6,476.0 | 1.120x | 20.740 / 13.559 / 13.474 |

These are short selection runs, not confidence intervals. The rule is an operational default-selection gate, not a weighted composite performance score. The 15-second selection numbers are not combined with the longer confirmation to manufacture a speedup.

## Confirmation without an environment override

After changing the fallback, the workspace is rebuilt and checked before further timing. The new release is launched with **no `DATABASE_MAX_CONNECTIONS` environment variable and no local `.env`**. Its startup log must report `max_connections=32` before load starts; that assertion passes.

Fresh reference and new-default processes then run in reverse order from the last selection pair, using 5-second discarded warmups and 30-second windows at 50 VU on all six operations. Startup-only info logging records the effective default; request logging remains disabled.

| Operation | Default 32 RPS | Reference RPS | Default / reference | Default p50 / p95 / p99 ms | Reference p50 / p95 / p99 ms |
|---|---:|---:|---:|---|---|
| CodeSystem validation | 5,387.8 | 4,948.8 | 1.089x | 8.671 / 13.807 / 17.819 | 9.568 / 16.141 / 19.844 |
| Lookup | 5,471.9 | 8,294.1 | 0.660x | 8.462 / 13.764 / 18.994 | 5.491 / 10.002 / 13.205 |
| ValueSet membership | 6,458.3 | 3,587.8 | 1.800x | 7.271 / 10.922 / 13.775 | 13.415 / 21.981 / 25.784 |
| Expansion | 4,559.3 | 927.4 | 4.916x | 10.157 / 14.975 / 17.890 | 55.221 / 63.668 / 68.036 |
| Subsumption | 5,093.4 | 9,211.5 | 0.553x | 9.110 / 14.943 / 19.855 | 4.952 / 9.006 / 12.156 |
| Forward translation | 6,160.5 | 6,965.2 | 0.884x | 7.513 / 11.627 / 15.157 | 6.749 / 11.574 / 14.146 |

Default 32 wins against this reference on validation, membership and expansion, but still loses on lookup, subsumption and forward translation. Margins such as the validation gain are not proof of universal superiority. All reference losses and tails remain visible. No overall score is calculated across unlike operations.

All twelve longer measurements pass. Across selection and confirmation there are **3,873,626 passing response checks, zero HTTP errors and zero failed checks**. The four-outcome subsumption, changing/missing validation codes, ValueSet exclusions, expansion pagination and translation assertions are unchanged. Reverse translation is not scored against the reference because its known reverse-result mismatch remains; it is not removed from term-squid's semantic checks.

## Resource observations

CPU/RSS use the same one-second process observer as the earlier reports (100% CPU = one core). These columns exclude PostgreSQL/VM/client costs; RSS is sampled, not a hard peak:

| Operation | Default server CPU % | Default sampled RSS MiB | Reference CPU % | Reference sampled RSS MiB |
|---|---:|---:|---:|---:|
| CodeSystem validation | 105.6 | 49.4 | 104.1 | 555.9 |
| Lookup | 106.4 | 49.7 | 103.8 | 557.0 |
| ValueSet membership | 120.3 | 49.9 | 115.5 | 622.9 |
| Expansion | 119.1 | 50.6 | 100.8 | 635.4 |
| Subsumption | 99.2 | 50.7 | 103.0 | 635.9 |
| Forward translation | 116.0 | 50.8 | 102.1 | 635.9 |

After the reference window, PostgreSQL reports 125.7 MiB of container memory and no benchmark-user sessions other than the inspection connection. After the default-32 window, while the pool is still alive, it reports **217.1 MiB and 32 sessions**. The database connection limit is 100. These are post-window snapshots, not peak memory or a precise per-session cost; shared buffers/cache and VM costs prevent treating the difference as a universal sizing formula. Server RSS alone is not total product memory.

Capacity is a maximum, not a minimum or an eagerly opened fixed allocation. Nevertheless, three busy replicas at 32 can leave insufficient room for import/admin connections on a database capped at 100. Keep an explicit smaller override where the total session budget requires it. Connection health/recovery, cold-plan admission, cache budgets and post-commit freshness are unchanged.

## Correctness and provenance

All **30 Rust tests**, including four database scenarios and a new public-config default/override test, pass. Clippy with warnings denied, formatting and all twelve benchmark harness tests pass. The live default-32 startup is checked directly; the same binary is then restarted with an explicit five and passes all seven local semantic preflights. Limits 1–64 and rejection of invalid settings remain in place. No SQLx health check or semantic assertion is weakened.

The host, database/content and pinned toolchain are unchanged from the [subsumption comparison](SUBSUMPTION_PERFORMANCE_REPORT.md): M3 Pro / 12 cores / 36 GiB, PostgreSQL 17.10 in a shared 2-vCPU Docker VM, k6 2.3.0, Bun 1.4.4, Node 24.16.0, and FHIRsmith revision `25a41f87711507789b361480ee4efae42df504d4`. Both deployments use synthetic 1.0.0 and R4 core 4.0.1, with the documented legacy normalization. Builds and intrusive profiles finish before timing. This is a non-dedicated local deployment comparison, not language-only throughput or full clinical conformance.

Raw records stay ignored under `.benchmarks/profiles/pool-selection/` and result prefixes `pool-screen-*` / `pool-confirm-*`, including the predeclared rule, all losing points, process samples, database snapshots, startup logs and explicit-five preflight. Both run groups have the same source/fixture harness fingerprints as earlier comparisons.

- Default-32 binary SHA-256: `05b0157e24736067c9f0fd4784669449d0786fbbd15825142e1ba29815fd4498`.
- Source archive SHA-256: `6f90f0cf6e41b89789ae3640cbd29ca36124034016981b6b86dbca7260d129c8`.
- The selection matrix uses the previous indexed binary `cfafe8f0f8099fe286622a280f7e19180aef164e4d919cc10cb01f2329952a3f` with explicit caps. The final binary changes only the server capacity fallback and its tests/documentation, not terminology algorithms.

To reproduce historical five-connection series, explicitly set `DATABASE_MAX_CONNECTIONS=5`; the new default must not be silently mixed with those results. To repeat the confirmation, start the new release with that variable unset and run:

```sh
bun benchmarks/bench.ts run --tests TS-CS01,TS-LK01,TS-VC01,TS-EX01,TS-SS01,TS-CM01 \
  --vus 50 --duration 30s --warmup 5s --strict --run-id UNIQUE \
  --dataset-label 'synthetic#1.0.0 + hl7.fhir.r4.core#4.0.1 (legacy v2 canonicals normalized at import)'
```

Use the identical harness/protocol and pinned content on the local reference. Capture effective settings, CPU/RSS and connection budget with each run. Larger/cold/clinical/mixed and arrival-rate workloads still need separate operational validation; this default does not solve the remaining engine/reference gap.
