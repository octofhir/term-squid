# Implicit membership execution tracker

November 3, 2026. Scope: supported implicit SNOMED whole-system / `isa` membership. Product/composite completion is outside this tracker.

- [x] Preserve the previous native source/binary and generated diagnostic data.
- [x] Add a reusable typed library definition and share exact/edition/default release selection.
- [x] Combine resolution, anchor validation, bounded traversal and membership into one SQL data statement.
- [x] Preserve fresh snapshots, update/rollback behavior, full traversal guards and pre-transfer page charging.
- [x] Test differential output, versions, committed/rolled-back changes, stale negatives, concurrent activation and error budgets.
- [x] Pass 81 Rust tests including database integration, formatting, Clippy and 29 harness tests.
- [x] Separate allocator/SQL profiles from production timing; retain invalid, primary and repeat series with explanations.
- [x] Record RPS, p50/p95/p99, zero HTTP/semantic failures and available backend CPU/RSS for all admitted windows.
- [x] Recheck the final production binary: 7/7 local preflights, 2/20 unchanged upstream preflights.
- [x] Publish evidence, unstable points, limitations and the next profiling/coverage work in [the report](../docs/IMPLICIT_MEMBERSHIP_REPORT.md).

No clinical speedup or composite score is established. See [the resume prompt](../docs/NEXT_SESSION.md) for the remaining work.
