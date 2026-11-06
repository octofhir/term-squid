# R4 translation safety and measured warm-read cost

## Task

Fix confirmed R4 ConceptMap wrong-positive semantics and reduce avoidable work on warm finite reads without weakening post-admission freshness, source identity, snapshot consistency or cancellation. Compare frozen baseline/candidate workloads against the pinned local FHIRsmith, not only against our own baseline.

## Plan structure

Preserve an immutable baseline, implement correctness and bounded execution changes, verify regressions, measure matched local workloads on a quiet host, then record results and remaining gaps.

## Execution plan

### Stage 1: Freeze baseline and implement the selected changes

**What to add/implement:**

- Freeze all tracked/nonignored inputs from the 379-test checkpoint and build an optimized API baseline with the unchanged tracked embedded assets. Preserve source and binary hashes.
- Require valid explicit R4 target equivalence; missing/null/invalid values must not default to `equivalent`.
- Resolve absent dependency/product namespaces to the effective source/target group system rather than wildcard or null. Preserve explicit criteria, actual source versions, reverse restrictions and unsupported unresolved contexts.
- Return `match.product.concept` as the R4 `Coding` output, while retaining the separate CodeableConcept input contract. Apply strict map validation before every JSON/package publication path, including the CLI.
- Add exact hierarchy Weak hints with a fresh unkeyed generation observation; use only the same confirmed snapshot. Keep keyed/cold SQL observation on folded, stale, missing or evicted paths.
- Reuse two bounded generation queue buffers across waves without changing queue capacity, batch detach boundaries, failure/cancellation ownership or snapshot freshness.
- After isolated allocation proof, avoid per-response gzip state below 1 KiB for known small bodies; retain large/streaming compression, MIME exclusions, explicit identity refusal, unsupported-coding errors and correct Vary behavior. Do not change benchmark client headers.
- Keep current Unicode/default-case policy unchanged; record the confirmed broader gap instead of claiming a one-line fix.

**Files to edit/create:**

- `crates/core/src/translation.rs` — required equivalence and effective criterion namespaces.
- `crates/ingestion/src/lib.rs`, `terminology.rs` — shared admission/regressions where required.
- `crates/cli/src/commands/import.rs`, `crates/backend/src/api/operations/translate.rs` — CLI publication guard and normative product output.
- `crates/backend/src/store/hierarchy.rs`, `generation.rs` — exact hints and bounded buffer reuse.
- `crates/backend/src/store/postgres.rs` — test-only observability if required.
- `crates/backend/src/transport.rs`, `main.rs`, `Cargo.toml` — request-aware size/encoding policy and explicit existing tower utility feature.

**Framework/library documentation:**

- [FHIR R4 ConceptMap definitions](https://hl7.org/fhir/R4/conceptmap-definitions.html) — required modifier equivalence and missing dependency/product system defaults.
- `docs/GENERATION_BATCHING_REPORT.md`, `docs/CONCEPT_READS_REPORT.md`, `docs/REQUEST_CONTEXT_PERFORMANCE.md` — freshness, cancellation and comparison constraints.

**Examples in existing code:**

- `crates/backend/src/store/concepts.rs` — exact Weak-hint protocol.
- Existing generation batching and translation integration tests — admission/freshness contracts.

**Verification commands:**

```sh
CARGO_TARGET_DIR=.benchmarks/target cargo check --locked --offline --workspace
cargo fmt --all -- --check
```

### Stage 2: Add correctness and lifecycle regressions

**What to add/implement:**

- Verify malformed equivalence cannot produce matches through core, JSON/package admission, inline/context resources, stored maps or cache-off paths. Preserve explicit unmatched/disjoint behavior.
- Verify default/explicit dependency and product namespaces, wrong-system negatives, missing effective context and reverse/version constraints.
- Test exact-to-folded hierarchy changes, delayed old observations, eviction/deletion/TRUNCATE, failed probes and mixed batches.
- Test bounded queue allocation reuse alongside existing overload, cancelled waiters and after-dispatch snapshot exclusion assertions.
- Test compression thresholds, decoded-byte fidelity, explicit `identity;q=0`/wildcard refusal, MIME exclusions, unknown-size streams, repeated headers and bodyless responses.
- Run the complete workspace and independent core boundary without weakening existing checks or official requests/goldens.

**Files to edit/create:**

- Core/ingestion translation tests and new backend translation safety tests.
- `crates/backend/src/exact_hierarchy_tests.rs` and generation/public-path regressions.
- Existing intentionally malformed fixtures only where they need an explicit intended equivalence.

**Verification commands:**

```sh
TERM_SQUID_TEST_DATABASE_URL='postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark' CARGO_TARGET_DIR=.benchmarks/target cargo test --locked --offline --workspace -- --include-ignored
CARGO_TARGET_DIR=.benchmarks/target cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR=.benchmarks/target cargo run --locked --offline --manifest-path tests/core-consumer/Cargo.toml
bun test benchmarks
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
```

### Stage 3: Profile and compare matched optimized builds

**What to add/implement:**

- Prepare an owned local database with the complete unchanged generated benchmark package and a separately owned pinned FHIRsmith data directory using that exact package.
- Verify all seven unchanged local semantic gates and preserve the generated-data/source hashes. No clinical/composite admission is implied.
- Profile allocation/query work separately from throughput. Build/test/profile work must finish before timing windows.
- Run unchanged local `TS-LK01`, `TS-SS01`, `TS-VC01` and `TS-CM01` against baseline, candidate and pinned FHIRsmith: 30-second windows at 1/10/50 VU, five-second per-test warmup, fixed pool/host and the same local protocol. Keep every failed/regressed point visible.
- Confirm the key single-client subsumption result in reverse order, and accept/reject optimization claims according to actual matched results rather than expectations.
- Retain raw outputs under `.benchmarks/spec-speed-20261106/`; stop only owned servers and remove only owned temporary databases.

**Verification commands:**

```sh
bun benchmarks/bench.ts fixtures
# Orchestrator supplies owned loopback URLs, immutable binaries and participant labels.
bun benchmarks/bench.ts run --strict --tests TS-LK01,TS-SS01,TS-VC01,TS-CM01 --vus 1,10,50 --duration 30s --warmup 5s
```

### Stage 4: Record actual delivery and remaining limits

**What to add/implement:**

- Update `docs/SPEC_SPEED_REPORT.md`, README/requirements, product plan and handoff with verified fixes, counts, hashes, complete matched measurements and regressions.
- State remaining Unicode/default-case/clinical/conformance limits explicitly. Do not claim complete R4 conformance, full ecosystem certification or clinical composite leadership.
- Preserve all prior changes. Do not commit, push or deploy.

**Verification commands:**

```sh
cargo fmt --all -- --check
git diff --check
```
