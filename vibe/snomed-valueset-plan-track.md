# Progress tracker for snomed-valueset-plan.md

Status: [ ] pending, [X] complete. Completion refers to this increment only.

- [X] Stage 1: Library primitives and operation boundary
- [X] Stage 2: Bounded native execution and index
- [X] Stage 3: Semantic/update/concurrency tests
- [X] Stage 4: Profile-driven SQL changes
- [X] Stage 5: Fresh paired measurements and final handoff

The final candidate passes 73 Rust tests, workspace Clippy with warnings denied, formatting and 29 unchanged harness tests. The generated 100k probe is isolated from the main corpus. The original main preflight remains 7/7 local and 2/20 clinical; no clinical composite claim is made.

The initial broad forced-hash experiment regressed a leaf and was rejected as a universal strategy. Boundary tests validate the cardinality-based branches. The ENOSPC/OrbStack-interrupted cohort is excluded; fresh baseline timings after volume recovery pass every semantic/HTTP assertion at all six points. The final candidate also passes all six native points with zero errors; expansion improves about 1.7x. Reverse-order membership repeats retain the primary 50-VU tail regression and its non-reproduction rather than replacing the primary point. The final ownership fix preserves borrowed finite plans; the short finite check improves 19,030 → 24,515 membership and 5,132 → 5,532 expansion RPS. Raw data/source archives/binaries remain ignored under `.benchmarks/native-valueset-20261103/`.

Official full-release acquisition instructions and the pinned FHIRsmith cache-version discrepancy are recorded in `docs/CLINICAL_DATA_SOURCES.md`. Missing authenticated archives do not block generated semantic tests or engine work.
