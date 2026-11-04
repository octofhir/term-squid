# Protected local FHIR-package import

Date: November 4, 2026. This extends the [public UI and standalone administration](UI_ADMIN_IMPLEMENTATION_REPORT.md); its React/UI-kit revision, toolchain, bootstrap/session policy and public routes remain applicable. It adds a complete local package upload/inspection/activation flow, without an OctoFHIR runtime, remote dependency resolution or CLI subprocesses.

## Format and admission

`POST /api/admin/jobs` accepts the original bytes with exactly one of these media types:

| Media type | Payload | File budget |
| --- | --- | --- |
| `application/fhir+json` | One UTF-8 R4 CodeSystem, ValueSet or ConceptMap | 16 MiB |
| `application/gzip` | A local gzip-compressed tar FHIR npm package, normally `.tgz` or `.tar.gz` | 32 MiB compressed |

Package layout requires `package/package.json`, a nonempty name/version and an explicit declaration of only FHIR **4.0.1** (`fhirVersions: ["4.0.1"]`, or the singular `fhirVersion`). Contradictory declarations fail. Only immediate `package/*.json` CodeSystem, ValueSet and ConceptMap resources are selected; other resource types and non-resource JSON are ignored. Every selected terminology resource passes the same structural/identity/graph checks as a single JSON upload. This is not full profile validation. Bundles are not unpacked as resource collections. A package containing no supported terminology resources fails.

Additional package limits:

- 128 MiB of decoded gzip bytes, including ignored members and data after the tar end marker; gzip trailers/CRC and concatenated gzip streams are consumed and checked.
- 4,096 archive entries exposed by the tar reader; 512-byte member paths, all under `package/` with ordinary path components.
- Regular files/directories only; no symbolic/hard links, special entries, parent/absolute paths or duplicate member paths. No members are extracted to the filesystem.
- 1 MiB metadata, 16 MiB per selected JSON member and 32 MiB of selected JSON in aggregate.
- 256 terminology resources, 50,000 concepts/definition entries in aggregate, no duplicate resource-type/canonical/business-version identities within a package.
- At most 64 bounded dependency declarations. Dependencies are recorded in inspection, but never fetched or installed automatically. The UI explicitly explains that prerequisite content must be installed separately.

The existing two concurrent upload requests, sixteen active jobs, one worker and 120-second upload/execution deadlines also apply. Large official core/THO packages can exceed these HTTP limits and should use the existing CLI. Native RF2/LOINC source archives still require their checksummed CLI manifest/multifile ingestion. RxNorm and LOINC accessory support is unchanged.

## Shared implementation and publication

`term-squid-ingestion::package` now owns the existing FHIR package parser, resource selection and narrow `hl7.fhir.r4.core#4.0.1` legacy-table normalization. The CLI downloader delegates to this parser with its established permissive package selection; its original commands, skip-existing package policy and 100,000-concept engine ceiling remain. CLI normalization regression tests still run through the shared code. HTTP applies the bounded R4 admission policy on that parser; there is no second parser in HTTP or the frontend.

The HTTP pipeline streams raw bytes into a private server-generated staging filename and records SHA-256, byte count, format and resource inventory. Existing filenames remain UUID `.json` storage keys for upgrade compatibility; that suffix does not select a parser. The persistent server-recorded `format` selects parsing. Parsing, decompression and graph checks run in `spawn_blocking`. The source checksum and complete inspection inventory are verified again before publication.

HTTP packages acquire canonical locks in deterministic resource-type/URL/version order. Every resource uses the existing shared metadata/concept/closure publication pipeline, immutable-version conflict checks and idempotence. All resources/projections, generation-trigger changes and the job success/result row commit in **one transaction**. A conflict in the last resource rolls back earlier resource writes; identical resources are reported separately from newly activated ones. Public readers cannot observe a partial package. Public terminology operations receive no new authentication queries or session dependency.

The job result contains real resource UUIDs/canonicals/business versions and `imported`/`unchanged` counts. The UI provides links to every activated resource, an inspection table and stage/error/cancellation states; it never invents a percentage. Local tables retain their own scrolling and the page remains bounded on mobile.

Session/Origin/CSRF checks still precede body consumption, staging and job creation for both media types. Source/job details remain private. Invalid archives return a sanitized JSON 422; operator logs retain diagnostics. Unsupported media/Content-Encoding and compressed byte budgets are rejected explicitly. Unknown API paths remain JSON responses.

## Lifecycle and upgrade

Apply **all** migrations through `term-squid-cli migrate`, including `20261104000005_admin_package`, before restarting the new backend. It adds a checked `admin_import_jobs.format` with default `fhir-json`; existing JSON jobs/history stay compatible. A downgrade refuses to drop the format column while any package jobs exist, avoiding reinterpretation of archives or silent history deletion. Explicitly remove relevant jobs and their private staging files only if an operator intentionally downgrades.

Restart recovery remains conservative: uploading/inspecting/queued/validating/publishing jobs become `interrupted`, never silently retried. Ready jobs survive if their private durable staging files survive; later activation revalidates them. Committed success is authoritative. Cancellation aborts uncommitted publication; cancelled/tampered/conflicting/interrupted imports do not activate partial resources. Staging cleanup, 24-hour ready expiry and seven-day terminal history retention apply equally to packages. The single admin-worker lease and separate bounded admin pool are unchanged.

Build frontend assets before compiling the RustEmbed backend:

```sh
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
cargo build --release --workspace --locked
# Apply migrations to the intended database, then restart its backend.
```

The UI-kit is still pinned to `5a17d7428e8da253fcd87375e45b41c89ff59997`, selecting `packages/ui-kit`; Node 24.16.0 and pnpm 10.11.0 are unchanged. No sibling server checkout is a build dependency. No package was published and no server-rs source was modified.

## Validation

All final checks passed on local instances. Raw archives, generated fixtures, screenshots and logs live only under gitignored `.benchmarks/ui-admin/` and `.benchmarks/package-upload-tests/`.

| Check | Result and raw evidence under `.benchmarks/ui-admin/` |
| --- | --- |
| Rust formatting | `package-fmt-final.log`, passed |
| Workspace Clippy, all targets, warnings denied | `package-clippy-final.log`, passed |
| Workspace tests including all PostgreSQL integration tests | **119 passed**, none ignored: backend 53, CLI 15, core 42, ingestion 9; `package-rust-final.log` |
| Frontend type checks, lint and unit tests | Passed; production build includes `tsc --noEmit`; `package-frontend-final.log`, `package-lint-final.log`, `package-frontend-tests.log` (**2 unit tests**) |
| Production embedded browser scenarios | **21 passed**; `package-delivery-e2e.log` |
| Actual process termination/restart | JSON and package jobs recovered as `interrupted`; no publication. Package crash pauses the ValueSet write after its earlier CodeSystem write, then kills the real process; same delivery log |
| Archive rejection and transaction regression | Unsafe paths/links/duplicates, malformed/empty/R5 packages, gzip CRC/decompression/member/resource/aggregate-record limits, idempotence, late conflict, checksum mismatch, cancellation after an earlier resource write, recovery and projection rollback; Rust log |
| Real browser package upload | Four independently generated two-resource packages at 390/1440 px, light/dark; exact uploaded SHA/bytes/inventory, absent before start, real activated IDs and public POST expansion; `uploads/`, `screenshots/package-*` |
| Accessibility and layout | No page overflow at 390/768/1024/1440 px; package tables retain readable column widths with bounded internal scrolling; keyboard ArrowRight scroll/focus verified at 390 px in both themes; dialogs/navigation/keyboard assertions preserved |
| Benchmark harness | **29 passed**, 34,217 assertions; `package-harness.log` |
| Original local semantic gates | **7/7**, unchanged; `../results/ui-admin-1793781182456795000/term-squid`; same fixture SHA-256 `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017` |
| Clean source export | Frozen pnpm install with empty `node_modules`, frontend build, then locked offline Cargo release build; no supplied static output or sibling checkout; `package-clean-install.log`, `package-clean-frontend.log`, `package-clean-rust.log` |

The clean install reused the normal content-addressed pnpm cache; the earlier UI increment separately verified a fresh store. All **150** build-source files match the final clean export; fingerprints are in `package-delivery-manifest.json`. The final browser driver exercised that clean-built binary. Backend SHA-256: `76b3916d13a26ee1516fa95a4d641bdec11cee2a13d1b594b857947363e42c7c`. Node/pnpm/Rust/browser versions remain those recorded in the base implementation report. The Vite main-chunk-size warning remains; no warning threshold was relaxed.

The first package browser attempt stopped at an ambiguous alert selector (both dependency and inspection alerts were present). The selector now targets the dependency alert specifically; all semantic assertions remain, and the final complete run above passed. The smoke driver resets its disposable login-attempt fixture after its two crash scenarios, before browser scenarios; the real persistent retry-limit behavior remains covered by PostgreSQL tests and is not disabled in the application.

The preview was left running at `http://127.0.0.1:5173/`, proxying its separate local UI-development schema on port 8081. Final health/session discovery returned 200 with administration configured and the anonymous administrator field null. No commits, pushes, package publication, deployment or changes to server-rs were performed.

## Remaining limitations

No automatic dependency downloads, registry/URL or arbitrary server-path input, package dependency graph resolution, Bundle import, unbounded official-package upload, native multifile browser upload, editing/deletion or new IAM integration. Package metadata is an untrusted declaration, not a signature or an attestation of clinical completeness. Existing terminology unsupported-operation errors remain explicit. This UI/ingestion increment supplies no new clinical coverage or comparative performance claim; the unchanged benchmark gates are semantic regression evidence.
