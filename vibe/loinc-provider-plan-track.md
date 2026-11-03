# LOINC primary-table and typed lookup tracker

November 3, 2026. Scope: numeric primary-table provider foundation, not complete clinical corpus or composite leadership.

- [x] Verify official primary-table/accessory scope, lifecycle and optional-field semantics.
- [x] Add bounded CSV decoding, strict streaming quote validation and exact source SHA/count verification.
- [x] Stage with COPY and activate immutable metadata, projections and provenance atomically.
- [x] Test same-version conflicts/idempotence, malformed input, publication rollback, concurrency and cancellation.
- [x] Preserve all raw columns, optional blanks and lifecycle; distinguish raw axis labels from LP Part identifiers.
- [x] Add library-owned primary filters/scope and implicit numeric membership.
- [x] Support scoped native LOINC plus SNOMED include/exclude/import composition with fresh snapshots.
- [x] Preserve R4 lookup property types and canonical-code fallback for missing display.
- [x] Pass 101 Rust tests including PostgreSQL integration, Clippy, formatting and 29 harness tests.
- [x] Import 100,005 generated rows with the release CLI and verify idempotent re-import / production HTTP behavior.
- [x] Recheck 7/7 local and 2/20 unchanged upstream preflights with no generated clinical substitution.
- [x] Finish the matched numeric CodeSystem matrix and restart repeat: seven admitted windows / 1,197,938 zero-error requests; retain the primary 50-VU loss and variable repeat.
- [x] Compare allocation churn separately: 2,040 warmup/measured requests, essentially unchanged calls/bytes.
- [x] Finalize the [report](../docs/LOINC_PROVIDER_REPORT.md), verify all 110 build inputs and stop temporary backend/load processes.
- [ ] Obtain a sustained native implicit-membership matrix on a quiet host; three attempts were interrupted by external builds and are explicitly unscored.

Next work: obtain the full pinned main table; add accessory Parts/answers/hierarchy and RxNorm. Existing source-table scope and explicit unsupported errors must remain until their complete source contracts are implemented.
