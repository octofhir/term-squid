# Progress tracker for inline-valueset-plan.md

Status: [ ] — pending, [X] — complete.

## Execution stages

- [X] Stage 1: Implement inline inputs and snapshots
- [X] Stage 2: Regression tests
- [X] Stage 3: Local protocol recheck and handoff

## Notes

- November 3, 2026: standalone Git project; preserve the previous uncommitted benchmark/UI work. No Arc rebase, commit or push. Original clinical pools remain unchanged; only loopback targets may receive load.

- Stage 1: workspace check and formatting pass offline. Request-local roots use SHA-256 cache identity, share bounded fresh snapshots with stored operations, and are never inserted into storage.

- Stage 2: all 51 Rust tests (including 13 database scenarios), Clippy with denied warnings and 29 Bun tests pass. Tests cover anonymous/named roots, typed filters, versions, intersections, concurrent cold sharing, committed/rolled-back dependencies, cache eviction, 256-KiB boundary and malformed mixed Parameters forms.

- Stage 3: release server passes all seven unchanged local gates and six actual inline HTTP checks. Original upstream coverage remains 2/20; five inline cases now reach dependency/evaluator checks rather than requiring a URL. Raw clinical gaps remain. Report: docs/INLINE_VALUESET_REPORT.md. No production traffic or new performance claim.

- Final combined release also rejects unsupported inline options and incorrectly typed/duplicate primitive inputs instead of silently ignoring them. All 57 combined tests and repeated release HTTP checks pass.
