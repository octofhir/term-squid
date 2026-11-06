# Progress tracker for spec-speed-plan.md

Status format: [ ] - pending, [X] - completed.

## Execution stages

[X] Stage 1: Freeze baseline and implement the selected changes
[X] Stage 2: Add correctness and lifecycle regressions
[X] Stage 3: Profile and compare matched optimized builds
[X] Stage 4: Record actual delivery and remaining limits

## Notes

- November 6, 2026: preserved the uncommitted 379-test checkpoint and froze 406 working-tree files. The intermediate and final artifacts each freeze 414 files; manifests, archives and immutable binaries remain under `.benchmarks/spec-speed-20261106/`.
- Required R4 equivalence, effective dependency/product namespaces and versioned product Coding are enforced across core, admission, CLI/packages and stored reads. Exact hierarchy hints still require a fresh database observation; two reusable bounded queue buffers preserve detach/cancellation rules.
- The first full test attempt exposed out-of-order fixture activation timestamps. Ordered fixture metadata and an explicit selector precondition corrected the test without changing production ordering or weakening response checks. Final workspace: 414 passed, zero ignored; Clippy/check/formatting, independent core, 38 harness/40,251 assertions and four adapter tests pass.
- The first dynamic compression candidate remains an intermediate artifact with all results retained. A one-request k6 capture proved Accept-Encoding is absent, so the Bun gzip allocation diagnostic is not the primary k6 request path. The final typed layer bypasses dynamic compression for no-header requests; thirteen transport tests pass.
- Final identity requested bytes fall 1.56–2.58% against baseline. The separate gzip-capable client reduces requested bytes 83.94–86.47% but increases allocation calls. Neither result measures retained memory or establishes CPU/throughput gains.
- Both timing phases complete: 66 official 30-second windows, 19,713,673 passing response checks and zero failures. Three separate folded global summaries include warmup: 182,974 requests / 365,948 checks. All original inputs, assertions, scorer and runner sources remain unchanged.
- Final primary throughput is below baseline in 11/12 points. All four 50-VU finite cases beat pinned FHIRsmith, while all four single-client cases lose. Reverse controls vary with phase/order; folded final throughput/tails regress. No stable overall speed or clinical composite claim is made.
- Both owned comparison databases and reference processes are cleaned up; original PostgreSQL/main benchmark database and unrelated processes remain. Raw evidence/provenance/cleanup records are retained. The actual six-file package manifest includes the prior generated folded hierarchy resource beyond the four-resource runner fingerprint.
- Updated report, requirements, README, product plan and handoff. Unicode SQL/Rust comparison, unknown case policy, clinical content and broader conformance remain open. Standalone Git: one user-requested commit collects the accumulated delivery; no rebase, push or deployment.
