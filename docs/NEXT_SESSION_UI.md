# Continue the branded public UI and administration

Latest continuation: [REQUEST_CONTEXT_REPORT.md](REQUEST_CONTEXT_REPORT.md). Public operation forms discard stale results after edits/navigation; catalog, concept and job tables retain readable columns in named keyboard-scroll regions without page overflow. The Git UI-kit pin, assets and standalone authentication/import architecture remain unchanged. See that report for the latest production browser and clean-build evidence.

November 4, 2026: the first implementation is present. Read [the implementation report](UI_ADMIN_IMPLEMENTATION_REPORT.md), [FHIRsmith comparison](FHIRSMITH_UI_ADMIN_COMPARISON.md), `AGENTS.md` and the current Git diff before changing it. Preserve all uncommitted/untracked files. Do not restart the Solid-to-React migration or replace the actual UI kit with local components.

The latest [R4 API implementation](API_IMPLEMENTATION_REPORT.md) covers all 70 declared inputs and passes **147 Rust/PostgreSQL tests** plus the unchanged harness. **23 production browser/API scenarios**, **29 harness tests** and **7/7 local gates** pass; the report contains evidence and exact limits. Apply `20261104000006_supplements`; the base UI/package reports remain the authority for authentication, upload formats, retention and Git UI-kit delivery.

## Current implementation

- React 19; Node 24.16.0 / pnpm 10.11.0; UI-kit Git SHA `5a17d7428e8da253fcd87375e45b41c89ff59997`, `packages/ui-kit`. Keep the package/lockfile and Vite/TypeScript alias fix together.
- Anonymous overview/catalog/detail/version/JSON/projection pages and read-operation tools; separate protected `/admin` and `/api/admin`.
- Argon2id bootstrap hash file, persistent HttpOnly sessions, Origin/CSRF enforcement, logout/revocation/expiry and global retry limits. No default password or external auth database.
- `term-squid-ingestion` shared by HTTP/CLI. One-resource R4 JSON and bounded local R4 FHIR `.tgz` imports, bounded staging/jobs, immutable versions/idempotence, atomic resource/projection/job commit and generation freshness. Native RF2/LOINC stay on the existing CLI manifest path.
- Migrations `20261103000004_admin` and `20261104000005_admin_package`; apply through CLI `migrate`. Frontend must build before compiling the embedded backend. CI artifact path is `crates/backend/static`.
- New generated mascot and distribution sizes in `frontend/public/branding/`; licenses in `frontend/public/licenses/`.

## Resume and verify

1. Inspect Git status and read the report's validation/limitations. No commit/push/deploy is authorized by this checklist.
2. Install frontend dependencies with `pnpm install --frozen-lockfile`; run typecheck/lint/unit tests and build before Cargo. A separate server-rs checkout is only a read-only design reference.
3. Follow the report for administrator bootstrap and browser-visible Origin. For development, backend port 8081 is proxied by Vite on `http://127.0.0.1:5173`; use that Origin for admin requests. Never copy local test credentials into tracked files.
4. Run workspace formatting/Clippy/tests including PostgreSQL, `bun test benchmarks`, and the embedded production browser driver. `benchmarks/ui_admin_smoke.py` requires an explicitly disposable loopback database, generated benchmark fixture package, release backend/CLI and Playwright Chromium. It kills a real import process to verify restart recovery.
5. Preserve all seven local FHIR gates and the benchmark dashboard. Load-test only local servers. UI acceptance is not a new clinical score or proof of R5/R6 support.

Local bounded R4 package ingestion is now implemented; read [PACKAGE_IMPORT_REPORT.md](PACKAGE_IMPORT_REPORT.md) before extending it. Remaining work includes package dependency resolution, Bundle/native multifile HTTP ingestion, richer content management and future authentication strategies. Keep unsupported workflows explicit. Prioritize only the next user-selected increment; do not silently resume engine/composite optimization while UI work is requested.

## Original acceptance contract

The original specification below is retained for reference; its descriptions of the starting Solid frontend and absence of auth APIs refer to the pre-implementation checkpoint.

### Implement the branded public UI and authenticated administration

Continue developing term-squid in the current repository. This is an explicitly selected UI/admin workstream. Implement and validate it; do not stop at a plan or a visual mockup. Communicate with the user in Russian; keep Git-facing code, comments, documentation and reports in English. Do not commit, push, publish a package or deploy without a new explicit request.

## Read and preserve the current state

1. Read `AGENTS.md`, `docs/NEXT_SESSION.md`, `docs/DEVELOPMENT_HANDOFF.md` and Milestone 6 in `docs/PRODUCT_PLAN.md`.
2. Read `docs/LOINC_PROVIDER_REPORT.md`, `docs/SNOMED_RF2_REPORT.md`, `docs/SNOMED_VALUESET_REPORT.md` and `benchmarks/README.md` before exposing ingestion through HTTP.
3. Inspect Git status and preserve all existing working-tree changes, including untracked engine/migration/test files. The latest recorded checkpoint is 101 Rust tests and 29 benchmark harness tests; recheck the actual checkout rather than assuming a commit or clean tree.
4. Inspect `frontend/`, backend routing/configuration/static serving, CLI ingestion and CI/build recipes. Coordinate shared ingestion changes if another agent is developing the engine; use a separate branch/worktree where appropriate.

term-squid remains a standalone terminology product with its own database and reusable Rust engine. Match the OctoFHIR server's UI, without embedding term-squid into its process, database or authentication database. Existing FHIR semantics, cache freshness, canonical/version resolution, atomic activation and explicit unsupported-operation errors must survive the rewrite. Keep raw screenshots, generated fixtures, uploads, logs and temporary artifacts under gitignored `.benchmarks/` during development.

## Product outcome: public browsing and private administration

Build a coherent terminology application with two clearly separated areas:

- **Public:** the landing/overview, CodeSystem/ValueSet/ConceptMap catalogs, search, resource details/FHIR JSON, version selection and terminology read workflows are usable without login. Use real API data and capabilities; preserve pagination and deep links. Make supported lookup, validation and expansion workflows usable, preserving other existing read operations. Do not invent counts, coverage, clinical data or performance badges.
- **Administration:** `/admin` contains imports/uploads, import-job history/details and the necessary content-management actions. Entry requires an authenticated administrator. Public navigation offers a small sign-in entry; authenticated administrators get an Administration entry, session identity and logout. Contextual upload/import buttons appear only after the server confirms admin capabilities. During session loading, do not flash admin actions to anonymous users.

Direct admin URLs must lead anonymous users to login and return them to a validated internal destination after login. Logout and session expiry remove admin controls and access. Public browsing continues to work when admin authentication is unconfigured or unavailable. No public registration or general IAM/user-management product is required for this increment.

**Read-only is defined by operation semantics, not HTTP method.** Public FHIR POST operations such as `$lookup`, `$validate-code`, `$expand`, `$subsumes` and `$translate` must remain available where supported. Do not put blanket authentication on every POST or add a session/database check to every public terminology request.

Native resources may have `content=not-present` while their concepts live in projections. Do not show this as an empty code system or infer a zero count from an absent JSON `concept` array. Do not download full systems/expansions into the browser to implement tables. Unsupported operations and unavailable clinical data must have honest user-facing states. R4 is the verified adapter; R5/R6 aliases do not establish conformance.

## Match the OctoFHIR server UI

Use the actual server as the design reference, not a generic dashboard template. On the current machine it is available read-only at `../server-rs`; read that repository's `AGENTS.md` before inspecting its source. Relevant files:

- `packages/ui-kit/src/shared/theme/` — colors, semantic tokens, typography, spacing and schemes.
- `packages/ui-kit/src/` — Shell, Sidebar, tables, inputs, buttons, dialogs, alerts, menus and related primitives.
- `ui/src/widgets/app-shell/AppShell.tsx` — shell/navigation patterns.
- `ui/src/pages/login/LoginPage.tsx` and its stylesheet — login composition.
- `ui/src/shared/api/hooks/useAuth.ts` — interaction reference, not permission to depend on the other product's backend.

At inspection, the server uses an ink navigation rail, restrained flat surfaces, marigold primary accents, semantic status colors, Lucide icons and light/dark schemes. Recheck the real tokens and screens; apply the same family of typography, density, tables, forms, focus states and interactions. Retain term-squid's product identity inside that family. Do not copy server-only navigation for features term-squid does not have.

The current terminology frontend is SolidJS. The inspected `@octofhir/ui-kit` is Apache-2.0, private, source-exported and requires React 19+. **Preferred implementation: migrate this frontend to React and reuse the real kit/components.** First inspect the kit's transitive imports, styles, aliases, fonts and build requirements. Record the inspected revision, dirty source hashes where relevant, and the selected component boundary.

**Use the actual UI kit as a Git dependency by default.** The user explicitly selected this delivery mechanism. `private: true` prevents npm publication; it does not prohibit installation from Git. The inspected server remote is `ssh://git@github.com/octofhir/server-rs.git`, and the package lives in `packages/ui-kit`, not at the monorepo root. pnpm supports selecting a Git subdirectory and combining it with a fixed commit:

```json
{
  "dependencies": {
    "@octofhir/ui-kit": "git+ssh://git@github.com/octofhir/server-rs.git#<COMMIT_SHA>&path:/packages/ui-kit"
  }
}
```

Resolve an actually reachable compatible commit, replace the placeholder and commit the resulting dependency/lockfile changes only when authorized. Pin/document a package-manager version that supports the selector; validate installation, source exports, `#` aliases, styles/fonts, transitive dependencies and React peer dependencies in an isolated clean checkout. The URL is an intended dependency shape, not a claim that installation has already been tested. Do not install the monorepo's root package or build the entire server application just to consume the kit. Do not use a floating branch. Document Git read access for developers/CI where required, without embedding tokens or private keys in manifests or lockfiles.

Reuse the real components and their update path; do not vendor/copy the kit merely because it is unpublished. No `file:../server-rs`, absolute sibling path, separately provisioned local checkout or assumed public npm publication may be needed to build term-squid. The sibling checkout is a read-only design reference; dirty reference changes cannot be fetched by a Git dependency. If the selected package has a concrete packaging defect, diagnose/document it and prefer a minimal reproducible integration fix; do not silently substitute a separate component system. Do not publish packages or modify/push the server repository without the user's explicit authorization. Official delivery references: [pnpm Git/subdirectory sources](https://pnpm.io/package-sources#install-from-a-subdirectory-of-a-git-repository), [npm private field](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#private).

Self-host the necessary assets/fonts. Verify the actual browser result against the reference on desktop and mobile, in light and dark modes, with keyboard navigation and visible focus. Preserve loading, empty, validation, permission and server-error states. Avoid fake dashboards, dead controls and decorative restyling disconnected from the reference.

## Real server-side admin authorization

There is no existing login/session/admin API in this checkout. Implement a maintainable standalone solution with established libraries. A local bootstrap administrator and server-managed sessions are an acceptable first implementation; a working independently configured OIDC integration is also acceptable. Do not make an OctoFHIR deployment a prerequisite or leave an environment-wide auth bypass.

- Configure/bootstrap credentials through a documented secure mechanism, store only appropriate password hashes, and ship no default password or public account-creation flow. Do not expose credentials or internal connection settings to the browser or logs.
- Use expiring/revocable sessions with HttpOnly cookies, appropriate SameSite/Secure behavior, logout invalidation and explicit development/production configuration. Handle invalid credentials, retry/rate limits and expired sessions. Do not use localStorage credentials or a frontend boolean as authorization.
- Enforce admin permission in backend routing/services before reading a large upload, creating a spool file, enqueuing a job or changing terminology data. Direct HTTP requests must not bypass the UI restriction. Authenticated users without admin permission, if supported, receive a forbidden response.
- Scope cookie/CORS/CSRF/origin behavior to the admin API. Current backend CORS is permissive; do not combine credentialed admin mutations with unrestricted origins. Keep the public terminology API's intended access and hot read path intact.
- Namespace admin APIs distinctly, for example `/api/admin`, with coherent JSON errors. Unknown API routes must return API errors rather than SPA HTML; UI deep links must still return the application shell.

## Working uploads and import jobs

Uploads/import currently exist only through the CLI. Build a real admin flow, reusing its ingestion semantics. Do not wire buttons to placeholder endpoints or directly to legacy `PostgresStore::create/update_*`: those methods do not provide the complete concept/projection rebuild pipeline.

Extract a shared Rust ingestion library/service where needed, called by both CLI and backend. Preserve the existing package/resource, RF2 and LOINC parsers, source checksums/counts, immutable-version conflict handling, transaction boundaries and generation updates. Do not shell out per upload or duplicate parsers in HTTP/frontend code. Keep existing CLI commands working.

Deliver at least one complete browser-to-activation flow for supported FHIR JSON or a local FHIR package: select file, inspect/validate, start import, observe real status/errors, and browse the activated result. Inventory the exact formats already supported; do not label arbitrary JSON, Bundle or archive variants as supported without implementing/testing their semantics. Native RF2/LOINC workflows must either use the real manifest/multifile ingestion path or be explicitly unavailable with a useful explanation. Missing RxNorm/LOINC accessory support must not appear as a working importer.

Use streaming uploads into server-managed staging, bounded file/decompression/record budgets, a bounded queue and worker concurrency. Do not accept arbitrary server paths or remote URLs from the browser. Long imports run as persistent jobs with actual stages, useful counts/results and sanitized errors. Show an indeterminate stage when a percentage is unknown. Handle double submission, cancellation, disconnects and process restart with an explicit recovery policy; the UI must not report success before activation commits. Unauthorized/failed/conflicting/cancelled imports must not leave partially published releases or stale positive/negative answers. Keep job/source-file details admin-only and document retention/cleanup. Administrative imports must not monopolize public-read database capacity or block the async runtime with parsing.

## Build, validation and delivery

Keep this a self-contained product build. Vite currently writes to `crates/backend/static`; RustEmbed consumes that directory during Cargo compilation. Build the new frontend before embedding it. Fix affected scripts/CI/artifact paths consistently: current `just build` builds Rust first, and the frontend CI artifact points at `frontend/dist` despite the configured output directory. Keep the separate benchmark dashboard/workflow intact.

Run appropriate frontend type checks, lint, meaningful component/browser tests, production build, Rust formatting/Clippy/workspace tests including database integration, and benchmark harness tests. Verify the production embedded application, not just Vite's dev server. Preserve the seven local FHIR semantic gates; never relax them for the UI rewrite. No performance claim follows from a visual change, and builds/browser work must not overlap final performance measurements.

Required end-to-end checks:

1. Anonymous visitors can use public catalogs, deep links and supported public GET/POST terminology operations, with no admin mutation controls.
2. Anonymous/expired/forbidden direct admin upload/import requests fail before any file/job/database side effect; session creation/logout/revocation behave correctly.
3. A real generated upload passes validation, completes a job and appears in public data after atomic activation. Malformed input, duplicate/conflicting versions, cancellation/restart and source failures preserve existing published data and freshness.
4. The application is visibly consistent with the OctoFHIR server on desktop/mobile and both schemes; forms, tables, dialogs, errors and keyboard flows are usable.
5. A clean checkout installs/builds with the locked Git UI-kit dependency and documented toolchain/access, without a separately provisioned `../server-rs` checkout or old generated assets. The backend serves the new embedded UI, while API errors remain JSON.

Update Milestone 6, development handoff and the relevant resume prompt. Write an English implementation report recording the kit/framework delivery decision, routes/access policy, admin bootstrap/session configuration, supported import scope, migration/upgrade steps, validation evidence and remaining limitations. Put raw screenshots and test/import artifacts under `.benchmarks/`. Proceed autonomously through implementation and validation; ask only for genuinely unavailable required information, and do not commit/push/deploy without a new request.

The November 5 final embedded artifact also passes the real immutable-edition browser flow: upload editions 1 and 2 through administrator jobs, log out, select edition 1 anonymously from the actual UI-kit Versions dialog, browse its projected concept and run lookup with that exact instance/version context. Dialog screenshots wait for content and the kit entrance transition; no CSS timing override is used. All 25 browser/API scenarios pass. The local preview at `http://127.0.0.1:5173` uses the final 206-test backend as of this checkpoint; local processes and credentials do not travel with Git.
