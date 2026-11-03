# Progress tracker for hierarchy-plan.md

Status: [ ] — pending, [X] — completed.

## Execution stages

- [X] Stage 1: Immutable closure index
- [X] Stage 2: Library contract tests
- [X] Stage 3: Projection tracking and cached reads
- [X] Stage 4: Freshness, admission and bounds tests
- [X] Stage 5: Matched measurements and handoff

## Notes

- November 3, 2026: existing working-tree changes are preserved. This is a standalone Git project; no Arc rebase, commit or push is performed. Raw fixtures, profiles and measurements stay under `.benchmarks/`.
- Stages 1–2 verified: the core compiles and all public hierarchy/core tests pass.
- Stage 3 verified: workspace check passes; SQLx applies the new migration to the isolated database.
- Stage 4 verified: all 38 Rust tests (six database scenarios), workspace Clippy with warnings denied, formatting and twelve harness tests pass. Coverage includes 10,001 folded concepts, a 100,001-concept SQL admission fallback, projection-only commits/rollback/TRUNCATE, schema guard, eviction, queued cancellation and concurrent atomic updates.
- Stage 5 verified: fourteen scored 30-second measurements pass 2,363,366 response checks. Both binaries use explicit pool 32. Original workload and reverse-order repeat retain reference losses; the separate folded reference fails strict direction probes and is not scored. Its responses and a case-only diagnostic remain recorded. Implementation, reproduction, limits, remaining work and handoff are updated; final formatting, harness tests and diff checks pass.
