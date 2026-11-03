# Native SNOMED ValueSet execution

## Increment scope

Use complete native RF2 projections for implicit whole/hierarchy sets and inline/stored/instance ValueSets. Keep membership independent of descendant enumeration and return bounded pages without constructing a finite release-sized Rust plan. Preserve snapshot consistency, versions, includes/excludes, imports and explicit unsupported operations.

## Execution stages

1. Add storage-independent implicit grammar/filter primitives and a common finite/native page/membership boundary.
2. Resolve bounded native definitions and compile projection-backed set operations in a fresh read-only snapshot. Add the reverse inferred-edge index; preserve immutable RF2 projection semantics.
3. Add generated PostgreSQL/HTTP fixtures for updates, versions, intersections, exclusions, page/memory bounds, cancellation, overload and snapshot consistency.
4. Profile broad and leaf cases, remove duplicate projection/count work, retain the small-set indexed plan and choose broad-set hashing from bounded actual cardinality. Batch static transaction setup without changing budgets.
5. Run the full Rust/Clippy/format/harness checks, preserve failed/interrupted diagnostics, then compare immutable binaries under the same sustained local workload. Update reports/handoff and document clinical acquisition sources.

Clinical archives, native LOINC/RxNorm, mixed providers, ECL/refsets, synonym search, translation compatibility and the full clinical composite remain later work. No commit/push/deploy is authorized by this plan.
