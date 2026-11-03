# Atomic subsumption reads — November 3, 2026

## Implementation

`$subsumes` now selects the CodeSystem, resolves both codes and checks both closure directions in one PostgreSQL statement. Previously the handler made two concept reads followed by one or two closure reads. Those statements could observe different committed updates.

The statement is the operation's snapshot boundary. It reads stored metadata and indexed concept/closure projections, without transferring resource or property JSON. There is no hierarchy cache, generation TTL or stale fallback. Updates must still write resource content, concepts and closure atomically; this change does not repair inconsistent storage writes. No new migration is required. The final query separates exact and case-folded resolution into mutually exclusive UNION ALL branches, preventing a measured full-system scan in the exact-case path. A generic case-insensitive plan can still choose a system-range scan; that performance limitation remains explicit.

Canonical/business-version parsing, deterministic default selection and exact instance UUID selection retain their existing paths. Both codes use the selected system's case policy; closure lookup uses their stored spellings. Missing systems/versions/codes and non-is-a hierarchies retain explicit errors. Equivalence is checked only after both codes exist. Reverse closure lookup is skipped when the forward relationship exists.

## Regression coverage

The new opt-in HTTP/database scenario covers all four outcomes, transitive ancestry, case-sensitive and case-insensitive codes, versioned canonicals, explicit/default versions, versionless instances, GET/POST, conflicting versions and missing resources/codes. Independent writer transactions exercise uncommitted changes, rollback, committed reversal and deletion. Four readers run while a writer repeatedly replaces concept spellings and closure in one transaction; every committed state has the same subsumption answer, so mixed-state reads are rejected.

CI now sets `TERM_SQUID_TEST_DATABASE_URL` and runs the workspace with `--include-ignored`. This enables the existing database integrations as well as the new scenario; previously the normal CI test step skipped them.

## Benchmark follow-up

The original TS-SS01 harness exercised only root-to-child `subsumes`. Its checker also accepted duplicate `outcome` parameters. The follow-up adds preflight and load checks for all four [R4 subsumption outcomes](https://hl7.org/fhir/R4/codesystem-operation-subsumes.html), requiring exactly one output with the expected `valueCode`. The original positive preflight and fixture content remain unchanged.

Load has a fixed 25% mix of `subsumes`, `subsumed-by`, `equivalent` and `not-subsumed`; each outcome rotates over the entire child pool. Missing-code/error cases remain in the HTTP/database integration scenario, not in successful-throughput measurements. Harness tests reject wrong outcomes, OperationOutcome responses, malformed output and duplicates; they check mixture and code coverage, including per-VU offsets and pool wraparound.

Local manifests now include the source fingerprint of the runner, case definitions and load script. A separate CI job runs the benchmark harness tests with pinned Bun 1.4.2. No server load runs in that job.

## Verification state

The initial attempt could not build because the Cargo cache lacked `config` and registry DNS was unavailable; sandbox permissions also prevented Docker/PostgreSQL access. The November 3 retry resolves the verification gap: dependencies are now available, and approved execution outside the sandbox can reach Docker and loopback services. The benchmark database was not running and was started in its own Compose project. This host has standalone `docker-compose`, not the `docker compose` plugin.

The workspace builds successfully. All 30 Rust tests pass with `--include-ignored`, including four database integration scenarios and two [legacy R4 package extraction tests](R4_CORE_COMPATIBILITY.md); workspace Clippy with warnings denied, formatting, `git diff --check` and all twelve benchmark harness tests pass. The two new subsumption harness tests first failed against the old harness: duplicate output was accepted and every sample had the same outcome. They pass with the four-outcome sampler and stricter checker. The retry uses Bun 1.4.4; the earlier harness run and CI pin use 1.4.2.

Repository migrations were applied through a temporary SQLx helper under `.benchmarks/`, preserving migration bookkeeping without installing `sqlx-cli`. The existing CLI imported the unchanged synthetic package. A temporary debug server then passed all seven local semantic scenarios, including the new four-outcome subsumption gate: 17 HTTP probes total. At that stage k6 was not installed, so an ignored Bun script invoked the existing `CASES`/`accepts` checks directly; this is a semantic-only check, not a timed benchmark or the full k6 runner. The temporary server was stopped afterward; the isolated database and its volume remain available.

`EXPLAIN (ANALYZE, BUFFERS)` on the combined source query confirms indexed concept access and composite index-only closure lookups for all four outcomes. The reverse closure branch is not executed for a forward match; neither closure branch is executed for equivalence. These single-query diagnostic plans do not establish HTTP latency or a throughput gain. Retry logs, probe responses and SQL plans remain under ignored `.benchmarks/subsumption-*-retry.*`.

The subsequent [performance report](SUBSUMPTION_PERFORMANCE_REPORT.md) records 26 sustained measurements and 3,099,603 passing response checks. At 10 VU the selected build reaches 2,922 RPS versus 1,107 for the previous build, but only 0.285x pinned local FHIRsmith throughput. Both the intermediate losing results and fresh repeats are preserved. Historical reports remain unchanged; the overall 3x reference and >12,000 simple-read RPS goals remain unachieved.

The later [default selection](POOL_DEFAULT_REPORT.md) raises the server pool fallback to 32 after all-six-workload measurements. Use an explicit five to reproduce the earlier fixed-capacity series.

## Reproduction and next measurement

With dependencies and local PostgreSQL available, apply the repository migrations and run:

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' \
  CARGO_TARGET_DIR=.benchmarks/target cargo test --workspace -- --include-ignored
cargo fmt --all -- --check
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --workspace --all-targets -- -D warnings
bun test benchmarks
```

Then start the local benchmark server and require `just bench-preflight --strict`. Inspect the combined query with `EXPLAIN (ANALYZE, BUFFERS)`, then compare TS-SS01 using the same current four-outcome harness on the previous build, this build and the pinned local FHIRsmith. Use 5-second warmup, 30-second sustained runs at 1/10/50 VU and the same content/pool settings. Collect RPS, p50/p95/p99, errors, CPU and RSS; keep builds/profiling separate from timing. Neither historical positive-only TS-SS01 results nor measurements from different hardware can replace a fresh matched comparison. The reference must pass the new preflight before receiving a score.
