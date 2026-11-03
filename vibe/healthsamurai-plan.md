# HealthSamurai Round 0 benchmark basis

## Task

Use HealthSamurai TX Benchmark revision `78bdffa769e21ac5d654f5dad412d1becf9d7e57` and its Round 0 archive as the primary external performance specification. Keep synthetic scenarios as regression diagnostics, not replacements for clinical workloads. Do not load-test the production Termbox endpoint.

## Execution plan

### Stage 1: Pin the reference and align orchestration

**What to add/implement:**

- Record the published Round 0 source/hash and timing protocol in `upstream.json`.
- Make upstream defaults match the original runner: 30-second windows, 1/10/50 VU, one global 5-second/10-VU warmup using its original script, and five seconds between tests. Preserve local defaults and all original upstream scripts/pools.
- Record protocol and source/pool fingerprints in manifests. Keep raw upstream preflight output, but block LK05 timing if its positive LK01 context fails, including when only LK05 was requested.
- Add a reference command that verifies the archived JSON checksum and reproduces the published five-server scores using the pinned upstream scoring module. Never insert our missing clinical results into the published leaderboard or call imputed values successful requests.

**Files to edit/create:**

- `benchmarks/upstream.json` — immutable Round 0 provenance/protocol.
- `benchmarks/bench.ts` — exported protocol/admission helpers, warmup and manifests.
- `benchmarks/healthsamurai.ts` — verified reference inspection and score reproduction.
- `justfile` — reference and strict local Round 0 recipes.

**Examples in existing code:**

- `benchmarks/bench.ts` — pinned checkout, local-only targets and result preservation.

**Framework/library documentation:**

- Pinned upstream `scripts/run.ts`, `scripts/lib/scoring.ts`, `site/src/pages/methodology.mdx`, `site/src/pages/data.mdx`, `site/src/data/round-0.json`.

**Verification commands:**

- `bun benchmarks/healthsamurai.ts reference`

### Stage 2: Test protocol and admission

**What to add/implement:**

- Test unchanged local defaults and canonical/custom upstream timing.
- Test negative-only false positives, mixed pass/skip/fail coverage and invalid archived data/checksums.
- Test extraction of real per-VU readings without substituting weighted/imputed values.

**Files to edit/create:**

- `benchmarks/healthsamurai.test.ts` — public helper tests using generated minimal data.

**Verification commands:**

- `bun test benchmarks`
- `git diff --check`

### Stage 3: Audit local coverage and document comparison limits

**What to add/implement:**

- Start the committed local server and run all twenty unchanged upstream preflight probes against the existing isolated dataset; retain missing/unsupported results without calling them speed measurements.
- Verify the canonical strict path refuses partial coverage and that the LK05-only path cannot score a missing SNOMED system.
- Document published per-operation numbers, hardware/content/version differences, required clinical data and licensing, available coverage and unmeasured gaps. Update the handoff/product plan; do not compare old synthetic RPS to Round 0 or stress production.

**Files to edit/create:**

- `docs/HEALTHSAMURAI_BASELINE.md` — provenance, protocol, coverage and reproduction.
- `benchmarks/README.md`, `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md` — primary baseline and next work.

**Verification commands:**

- `bun test benchmarks`
- `bun benchmarks/healthsamurai.ts reference`
- Run the complete local upstream preflight and the strict/negative-only admission checks.
- `cargo fmt --all -- --check`
- `git diff --check`

### Stage 4: Local graphs and complete-cohort scoring

**What to add/implement:**

- Generate the pinned original site into `.benchmarks/ui-site` with a separate local page, matching historical synthetic charts, published composite and local coverage audit. Missing raw artifacts stay missing.
- Add complete-cohort scoring through the original module, requiring local term-squid/Termbox/FHIRsmith identities, full matrices, original timing, revisions, matching clinical archive attestations and environment/harness records. Preserve weighted versus actual readings.
- Add a clinical manifest template that cannot pass until completed and attested; expose scoring and site commands in `justfile`.

**Files to edit/create:**

- `benchmarks/composite.ts`, `benchmarks/round0-dataset.template.json` — cohort admission and scorer output.
- `benchmarks/healthsamurai.ts`, `benchmarks/bench.ts`, `justfile` — commands and capture metadata.
- `benchmarks/ui.ts`, `benchmarks/ui/local.astro`, `benchmarks/ui/LocalChart.svelte` — generated local site and real charts.

**Examples in existing code:**

- `benchmarks/bench.ts` — result layout and admission.
- Pinned upstream `site/src/layouts/Base.astro`, `site/src/components/EChart.svelte` — original site/chart integration.

**Verification commands:**

- `bun benchmarks/ui.ts build`
- Execute an isolated generated-fixture cohort through the pinned original scorer; never write fixture output to the dashboard artifact path.

### Stage 5: Score/site tests and priority handoff

**What to add/implement:**

- Test clinical attestation, complete/mismatched/failed matrices, point ownership, real chart values and missing artifacts.
- Verify the page on desktop/mobile: twelve actual SVG charts, no page overflow, no JavaScript errors/external requests, and working published-result navigation.
- Document the requested HTS read-only audit with pinned source evidence and a score-first backlog. Update the product priority to composite leadership and bring benchmark-required clinical/SNOMED work forward.

**Files to edit/create:**

- `benchmarks/composite.test.ts`, `benchmarks/ui.test.ts` — public behavior tests.
- `docs/HTS_FEATURE_GAPS.md`, `docs/HEALTHSAMURAI_BASELINE.md`, `docs/PRODUCT_PLAN.md`, `docs/DEVELOPMENT_HANDOFF.md`, `benchmarks/README.md` — findings, commands and next work.

**Verification commands:**

- `bun test benchmarks`
- `bun benchmarks/ui.ts build`
- Local Playwright desktop/mobile inspection and screenshot review.
- `cargo fmt --all -- --check`
- `git diff --check`
