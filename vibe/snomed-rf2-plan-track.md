# Progress tracker for snomed-rf2-plan.md

Status: [ ] — pending, [X] — complete.

## Execution stages

- [X] Stage 1: Native storage and read adapters
- [X] Stage 2: Native read regression tests
- [X] Stage 3: Streaming RF2 importer
- [X] Stage 4: Importer tests
- [X] Stage 5: Local release smoke and handoff

## Notes

- Standalone Git project. Preserve all earlier uncommitted benchmark, inline and UCUM work; no rebase, commit or push.
- Clinical source archives have not been supplied. Use original generated fixtures to verify the implementation, not to claim a clinical score or full-release import performance.

- Stages 1–2: native storage/read adapters compile and all 59 current Rust tests pass. Two new PostgreSQL tests exercise native knowledge, version/instance selection, all outcomes, active inferred edges, freshness and the 8,192-entry traversal bound. Existing TRUNCATE projection tests remain unchanged; native references to concept codes are validated by the loader rather than adding incompatible projection-table foreign keys.

- Stage 3: workspace check passes. CLI streams four checksummed Snapshot files through a two-chunk channel and transaction-local COPY staging, validates references/dialect/FSNs and bounded DAG depth, and publishes native rows atomically. Clinical source files are not bundled or downloaded.

- Stage 4: all 64 Rust tests pass, including nineteen database scenarios; Clippy denies warnings. Generated RF2 fixtures cover preferred terms with quotes/backslashes/Unicode, CRLF, provenance, immutable/idempotent versions, fifteen invalid source/graph cases, publication failure rollback and cancellation/retry. No clinical corpus is present.

- Stage 5: release CLI/server smoke passes up/down/up, dry-run, import/reimport, checksum rejection and thirteen HTTP checks in a disposable synthetic schema. Downgrade with native releases is explicitly refused. First-party CLI migrate removes the external-tool dependency. The additive main migration preserves all corpus counts; unchanged local gates pass 7/7, clinical preflight remains 2/20 with no native clinical data loaded. NEXT_SESSION.md and benchmark/product handoff describe the next native ValueSet provider, not a fabricated score improvement.

- Native expressions are explicitly unsupported, not invalid scalar-code answers; the public core test and native/non-native HTTP checks pass. Final live-server snapshots use a fresh immutable filename rather than overwriting a running executable.
