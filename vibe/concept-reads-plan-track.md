# Progress tracker for concept-reads-plan.md

Status: [ ] — pending, [X] — completed.

## Execution stages

- [X] Stage 1: Immutable projection index
- [X] Stage 2: Public library tests
- [X] Stage 3: Bounded PostgreSQL snapshots
- [X] Stage 4: HTTP/store freshness and limits
- [X] Stage 5: Matched measurements and handoff

## Notes

- November 3, 2026: preserve all existing changes. Standalone Git project; no Arc rebase, commit, push or deployment.
- Pre-implementation forced-generic EXPLAIN probes use exact/folded bitmap index bounds (0.033–0.138 ms in isolated samples). No full-system scan is observed. Preserve original SQL as fallback; target repeated projection decoding and per-request protocol cost instead. Raw plans are under `.benchmarks/profiles/concept-reads/`.
- Stages 1–2: core check and all 30 public core tests pass; payload/property values and provider-defined comparison keys are preserved.
- Stage 3 verified: the workspace compiles. Existing migrations/SQL fallback remain unchanged; decoded payload charges are checked incrementally in blocking preparation.
- Stage 4 verified: all 47 Rust tests (ten database scenarios), Clippy with warnings denied, formatting and twelve harness tests pass. Coverage includes atomic metadata/payload updates, versions/instances, complete/fragment behavior, stored properties, PostgreSQL Unicode keys, queued cancellation, eviction, count/property/decoded-memory/metadata SQL markers and post-marker freshness.
- Stage 5 verified: 21 scored measurements pass 4,399,050 response checks with explicit pool 32 and identical workload/content labels. Both loaded operations exceed 12k RPS; lookup preserves its primary/repeat 16,949–21,265 range, low-concurrency reference losses, validation tail regression and increased RSS. Reports/handoff/reproduction are updated. Final Rust tests, Clippy, formatting, harness tests and whitespace/diff checks pass; temporary servers are stopped. No overall 3x or clinical/mixed-load claim is made.
