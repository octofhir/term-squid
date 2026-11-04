# Public UI and standalone administration

November 4, 2026. Base checkpoint: `b426651`. No commit, push, publication or deployment was performed. term-squid retains its own PostgreSQL database and reusable Rust terminology engine.

The subsequent [local package import increment](PACKAGE_IMPORT_REPORT.md) extends the JSON-only format inventory and validation checkpoint below. Read it for current `.tgz` support and migration `20261104000005_admin_package`.

## React and OctoFHIR UI-kit delivery

The Solid application is replaced by React **19.3.1**, React Router **7.18.5** and the actual source-exported `@octofhir/ui-kit`. The dependency is pinned to reachable server-rs commit **`5a17d7428e8da253fcd87375e45b41c89ff59997`**, using pnpm's combined commit/subdirectory selector:

```json
"@octofhir/ui-kit": "git+ssh://git@github.com/octofhir/server-rs.git#5a17d7428e8da253fcd87375e45b41c89ff59997&path:/packages/ui-kit"
```

Toolchain: Node **24.16.0**, pnpm **10.11.0** (`packageManager` and CI pinned), TypeScript **5.8.3**, Vite **7.1.7**, Rust **1.99.0** in local verification. The frozen pnpm lock resolves the selected package to the fixed GitHub source archive and preserves its subdirectory. A new store/source export installs and builds without a sibling `server-rs`, old `node_modules` or generated static files. No server monorepo root package/application is installed or built to consume the kit. The selected commit is publicly fetchable through codeload at verification time; re-resolution of the SSH Git specifier requires Git read access and trusted GitHub SSH host configuration. Developers/CI for restricted repositories must provision read credentials through their normal SSH agent/credential manager; credentials must never enter a manifest or lockfile. The current public frozen archive installation required no embedded credentials.

The package has a concrete integration defect: its `#/*` imports map uses Node-invalid `#/…` specifiers, and its original monorepo alias configuration is absent in consumers. `tsconfig.json` maps those imports to the installed package's source. Vite supplies the same alias both to normal resolution and to the esbuild dependency optimizer. Excluding the kit from optimization alone is insufficient: its CommonJS transitive dependencies then fail named-export resolution in development. The explicit optimizer resolver fixes development and production reproducibly. This is a consumer integration fix; no component source was copied, vendored, patched or pushed upstream.

The kit itself was clean at the selected commit. The sibling reference had unrelated dirty changes, including AppShell; it was inspected read-only. Reference SHA-256:

| Reference | SHA-256 |
|---|---|
| `ui/src/widgets/app-shell/AppShell.tsx` | `8f030b07cb4ad45a3a4189e02ef90ee366126c685ba5968c9767535a53186e80` |
| `ui/src/pages/login/LoginPage.tsx` | `70080b5f63286f3f06b0e362c0f3a129baa5a6b1a4e521381ac73f4a9accac03` |
| Login stylesheet | `b8576a79668060edd8da1cd935a1476d03b590e94c1f18143484769406dce17e` |
| `useAuth.ts` | `a8214108eb6a7db585e6a9dc221c1aee435700ca9107c16a098bfd2bed599010` |

The application directly uses UIProvider/theme tokens, ink Sidebar, Shell, LocationBar, Table, Button, ActionIcon, TextInput, PasswordInput, Select, Card, Alert, Badge and Modal. It retains the kit's IBM Plex Sans body, JetBrains Mono headings, flat surfaces, marigold accents, semantic status colors and theme provider. Long titles and breadcrumbs wrap/shrink within the shell; wide tables/JSON have local scrolling. The mobile rail opens from the header. The skip link precedes navigation; required inputs supply explicit accessible names/required semantics because the kit otherwise incorporates the visual asterisk into field names.

Fonts/assets are self-hosted by Vite and RustEmbed. Apache and font OFL notices are shipped at `/licenses/`; the build also assembles production dependency license texts in `THIRD_PARTY.txt`. No OctoFHIR backend, authentication database, runtime or sibling checkout is needed.

## Routes and access policy

| Routes | Access |
|---|---|
| `/`, `/catalog/:kind`, `/catalog/:kind/:id`, `/operations` | Anonymous public browsing |
| `/api/catalog/{CodeSystem,ValueSet,ConceptMap}` and resource details | Public bounded metadata pages |
| `/api/catalog/CodeSystem/{id}/concepts` | Public 25-row projection pages; never a release-sized browser download |
| Existing `/r4` reads and supported GET/POST operations | Public, with unchanged terminology dispatch and generation/snapshot checks |
| `/login` | Anonymous sign-in; only validated internal `/admin` return destinations |
| `/admin`, `/admin/jobs/:id` | Administrator UI guard backed by the session API |
| `/api/admin/session`, `/api/admin/login` | Public session discovery / bounded, origin-checked login |
| `/api/admin/logout`, `/api/admin/jobs`, job details/start/cancel | Server-authorized administration |

Catalog totals are filtered database counts. Business versions resolve to installed instance IDs. Full FHIR JSON is fetched only when explicitly opened, independently of table pagination. `content=not-present` never implies an empty projection. R4 is the verified adapter; existing R5/R6 aliases are retained without advertising their conformance. The overview does not fabricate coverage/performance claims or substitute zero for a database error.

Admin controls require a confirmed identity and `import` capability; they are absent during initial session loading. Expiry timers, focus/poll rechecks and unauthorized API responses remove access. Request sequencing ignores late session responses after logout or expiry. Authentication discovery failures preserve public browsing and cannot create a session-fetch retry loop. Read-only terminology POST operations do not require sessions. No authentication/session database query was added to their server hot path. Admin CORS is separate from permissive public terminology CORS; unknown API routes return JSON, while frontend deep links serve the shell.

## Bootstrap and session configuration

Apply all migrations before starting the backend:

```sh
cargo run --locked --bin term-squid-cli -- --database-url "$DATABASE_URL" migrate
```

Migration **`20261103000004_admin`** adds persistent sessions, a login retry-budget row and import jobs. Startup does not migrate automatically. The reverse migration deletes only administration state; back it up if its history is needed.

Generate an Argon2id PHC hash through stdin, keeping plaintext out of arguments/history. For example, in a service-owned private configuration directory:

```sh
umask 077
python3 -c 'import getpass; print(getpass.getpass("Administrator password: "))' \
  | target/release/term-squid-cli hash-admin-password > /private/service-config/admin.phc
```

The password helper requires 12–4096 UTF-8 bytes and no database connection. No default password, registration route or external IAM database is supplied.

| Variable | Behavior |
|---|---|
| `ADMIN_PASSWORD_HASH_FILE` | Path to the Argon2id PHC file; absent disables administration |
| `ADMIN_USERNAME` | Bootstrap identity; defaults to the name `admin`, never a default password |
| `ADMIN_ORIGIN` | Exact browser-visible HTTP(S) origin, without trailing slash/path; required when enabled |
| `ADMIN_SESSION_SECONDS` | Fixed session lifetime, default 28800; allowed 30–86400 |
| `ADMIN_STAGING_DIR` | Durable private service-owned directory; default `.term-squid-staging` |

Use HTTPS at the browser/reverse proxy in production. A non-HTTPS origin is accepted only for loopback development. `Secure` follows the configured HTTPS origin; cookies always use HttpOnly, SameSite=Strict and Path=/api/admin. For Vite development the browser-visible origin is `http://127.0.0.1:5173`, although its API proxy targets backend port 8081.

Sessions store a SHA-256 digest of a high-entropy random bearer, an independent CSRF token, identity, credential fingerprint and expiry in term-squid's database. Logout deletes the server session. Expired sessions are rejected immediately; restarting after changing the bootstrap hash or identity rejects old sessions. The store retains at most twenty login sessions. Login has a persistent global five-attempt/minute budget and one concurrent password check; it deliberately ignores untrusted forwarded IP headers. This simple single-administrator budget is not general IAM or a multi-user login policy.

Mutations require both the exact Origin header and the session's CSRF header. Authorization executes before streaming a body, creating staging files or inserting jobs. Session endpoints use no-store. Invalid configuration or unavailable worker initialization fails closed for administration while public browsing stays available; inspect the server log for the cause. There is no environment-wide authorization bypass.

## Supported imports and lifecycle

The HTTP uploader accepts exactly **one UTF-8 R4 CodeSystem, ValueSet or ConceptMap JSON resource**, sent as `application/fhir+json`. Limits: **16 MiB**, **50,000 CodeSystem concepts or ValueSet/ConceptMap definition entries**, two concurrent upload requests, sixteen active staged/queued/executing jobs, one job worker, 120 seconds per upload and execution. HTTP compression, archives, Bundles, remote URLs and server filesystem paths are not accepted. Consequently there is no HTTP decompression/extraction path to evade the file budget.

Inspection validates metadata, canonical/version shape, applicable definition structure/budgets and CodeSystem graph preparation. It is not a complete FHIR StructureDefinition/profile validator or proof of clinical completeness. Syntactically valid definitions can still use unsupported terminology expressions, which the existing read engine rejects explicitly.

`term-squid-ingestion` shares resource publication and concept/closure preparation between the CLI and backend. Package commands also reuse its concept/closure pipeline and canonical admission lock. Existing CLI package/native formats and commands remain available; their resource-skip policy is preserved. Single-resource CLI creation retains the engine's 100,000-concept ceiling and its duplicate-create error. No upload shells out to the CLI or calls legacy JSON-only store create/update methods.

The worker verifies the original staged SHA-256 again, re-inspects counts, prepares concepts/closure off the async runtime and writes resource/projections within one transaction. Canonical/business-version locks coordinate CLI and HTTP admission, including NULL versions. Identical content is idempotent; changed content under the same identity fails. The success/result job row commits in the same transaction as publication. Generation triggers, atomic snapshots, rollback and cross-process freshness remain in force.

Stages are `uploading → inspecting → ready → queued → validating → publishing → succeeded`, with explicit `failed`, `cancelled` and `interrupted` terminal states. Counts represent inspected concepts/definition entries and uploaded bytes, not invented percentages. Start is a compare-and-set, so repeated submission of one job cannot publish twice.

An upload connection failure/timeout never starts activation. Disconnected `uploading`/`inspecting` rows are reclaimed after ten minutes. Once queued, a job persists independently of browser disconnect, logout or session expiry; explicit cancellation is required. Queued/ready cancellation is immediate; executing cancellation drops the publication future and rolls back. A cancellation racing final commit serializes on the job row: committed success is authoritative. SQLx cancellation can drain database work until the 30-second statement / five-second lock timeout; it does not promise instantaneous PostgreSQL cancellation.

On restart, unfinished upload/queued/executing jobs become `interrupted` and are not automatically retried. Ready inspections survive for 24 hours. A schema-scoped database lease permits one configured administrator worker; additional public replicas can run without administrator configuration. Loss of the lease disables administrative access until restart. Administration has a separate four-connection pool, including its lease, so imports do not take the public pool's connection slots. PostgreSQL CPU, disk, WAL and overall process RSS still need operational capacity planning.

Maintenance is checked against wall time once a minute between jobs; a running job can defer it until its bounded execution finishes. Terminal staging files are removed after execution/cancellation cleanup; unstarted ready files expire after 24 hours, terminal job history after seven days. The UI/API lists the latest 100 jobs. Files, checksums, job errors and history remain admin-only. File names are generated UUIDs; original client paths are not stored or exposed. Failures returned to clients are sanitized; detailed diagnostics remain operator logs.

Native SNOMED RF2 and primary LOINC remain the existing checksummed multifile/manifest CLI flows. They are explicitly unavailable in this uploader. RxNorm and LOINC accessories remain unsupported. No clinical corpus or benchmark-score claim follows from this increment.

## Build and upgrade

Install the documented Node/pnpm toolchain and Git access, then:

```sh
cd frontend
pnpm install --frozen-lockfile
pnpm run typecheck
pnpm run lint
pnpm run test
pnpm run build
cd ..
cargo build --locked --release --bin backend --bin term-squid-cli
```

Vite writes to `crates/backend/static`; build it **before Cargo compiles RustEmbed**. `just setup`, `just build`, `just build-release` and `just bench-server` now observe that ordering. Benchmark server startup also installs the frozen frontend dependencies when needed. CI's Rust jobs consume the frontend-static artifact at the actual output path. Release-server uses the same pinned toolchain/frozen installation. The separate terminology benchmark dashboard/workflow remains unchanged.

The browser CI job builds a release after receiving frontend assets, generates synthetic fixtures and runs `benchmarks/ui_admin_smoke.py` against disposable loopback PostgreSQL. The driver performs a real process-kill/restart test, then Playwright workflows and the unchanged seven local FHIR gates. Locally set `CHROMIUM_PATH` only when deliberately using an already-installed Chromium; otherwise run `pnpm exec playwright install chromium` first. No browser is needed to build or run the product.

## Mascot asset

The built-in image generation tool created a new term-squid app icon, using the previous mascot and server app icon as visual references. Final project assets live in `frontend/public/branding/`: `term-squid.png` (master), `icon-512.png`, `icon-192.png`, `apple-touch-icon.png` and `favicon-32.png`. `sips` generated distribution sizes without redrawing the image. Existing original mascot assets are retained. The new image is used in the rail, login and favicon.

Final generation prompt:

> Use case: logo-brand. Generate a new production app icon for term-squid, a standalone FHIR terminology server. Use the two reference images only as design references: image 1 is its current playful sombrero-wearing squid/coder mascot; image 2 is OctoFHIR's restrained marigold and ink geometric app icon. Redesign the term-squid mascot to belong to the OctoFHIR UI kit: very simple bold flat geometric silhouette, friendly squid with compact sombrero outline and a few strong curved tentacles; retain the sombrero and squid identity, simplify heavily for legibility at 24px. Rounded square tile, solid marigold #F2B630 background and ink #16181D mark. Centered, generous internal margins, extremely crisp edges, no gradients, no shadow, no 3D, no texture, no blue/purple/red/green, no flame, no face of a human. No text, no letters, no registered symbol, no code brackets. Single icon only, square 1024x1024. The result should feel as restrained and clear as the second reference but unmistakably be the term-squid squid mascot.

See the separate [FHIRsmith comparison](FHIRSMITH_UI_ADMIN_COMPARISON.md) for the pinned reference's public UI, startup library configuration and lack of an equivalent authenticated terminology upload/job workflow.

## Validation evidence

All checks below ran locally; CI definitions were updated and syntax-checked but were not executed remotely.

| Check | Result |
|---|---|
| Frontend typecheck, ESLint, production build | Passed |
| Frontend unit tests | 2 passed, including canonicalized return-destination traversal cases |
| Rust workspace with PostgreSQL integration / `--include-ignored` | **110 passed**, zero failed/ignored: backend 50, CLI 15, core 42, ingestion 3 |
| Workspace Clippy, all targets, warnings denied; rustfmt | Passed |
| Benchmark harness | **29 passed / 34,217 assertions**; assertions unchanged |
| Embedded release browser suite | **20 passed** |
| Original seven local FHIR semantic gates | **7/7 passed**, unchanged fixture SHA `10301f17660900999fb4769d2c8f24df5c1da7691a8e5a316877160f27cbc017` |
| Clean source export / fresh pnpm store | Frozen Git-package install and frontend-before-Cargo release build passed; no sibling checkout or previous static output supplied |
| Process-kill recovery | Passed: an import paused before commit was killed; restart persisted `interrupted`, and its canonical was absent publicly |

Browser verification used Playwright **1.64.1** with local Chromium **151.0.7922.34** (explicit cached executable override), against the actual optimized RustEmbed backend on loopback port 8082, not Vite. The database was PostgreSQL **17.11**, with isolated `ui_admin_e2e` / `ui_admin_dev` schemas and separate disposable test schemas. No clinical data or public terminology server was loaded/tested by this increment. The local benchmark corpus was not used for upload mutations.

The suite covers anonymous catalogs and GET/POST lookup, CodeSystem/ValueSet validation, expansion, subsumption and translation; proper unknown-API errors; public deep links and concept pagination; dialog focus/Escape, keyboard skip navigation, mobile rail and absence of administrative controls; actual generated-file upload/checksum/byte/count inspection, absence before activation, job success and public lookup after activation; server logout and 30-second session expiry; unavailable auth; delayed-session-response suppression and traversal-safe login returns. Desktop/tablet/mobile pages at **390, 768, 1024 and 1440 px** pass in both schemes with no page/main/header horizontal overflow. Both schemes also pass resource-dialog sizing and keyboard focus checks on mobile and desktop. Runtime assets/fonts are same-origin, with no application JavaScript errors in the observed public workflow.

Database tests additionally cover rejected upload bodies before polling/side effects, wrong credentials/retry limits, expired/revoked sessions, Origin/CSRF refusal, immutable conflicts, duplicate/idempotent and concurrent unversioned imports, corrupted staged bytes, cancellation before and during publication, injected projection-write failure, rollback, restart classification and freshness after a warmed negative. A forty-version catalog test verifies filtered totals, non-overlapping 25/15 pages, business-version selection, literal wildcard search and projection-only concepts despite `content=not-present`. Catalog SQL selects page IDs before reading resource JSON and avoids materializing complete concept payloads for pagination.

Final admitted evidence is under `.benchmarks/ui-admin/`: `delivery-e2e.log`, `rust-delivery.log`, `clippy-delivery.log`, `fmt-delivery.log`, `frontend-return-*`, `harness-tests.log`, `clean/clean-install.log`, `clean/clean-lock-recheck.log`, `clean-release-delivery.log`, `screenshots/`, `uploads/` and `smoke/backend.log`. Earlier failed/pre-fix attempts are retained and excluded from the passed results above. Raw files and private credentials must not be committed or published. The original seven-gate output is also retained under `.benchmarks/results/ui-admin-1793741937529890000/term-squid/`.

Final clean-export release SHA-256:

- Backend: `3dfb7d8b76a02a33444f49f0ae4230c6da770b1b335c99dd2d8153c68aa52bd6`.
- CLI: `3693ceee81cf6b998d320c2ad4e8976e7f12c920b83bf9826cdbc00b50f84d43`.

## Remaining limitations

This is a bounded first administration increment, not complete terminology authoring or clinical/FHIR conformance. Browser package/archive/Bundle and native multifile imports, editing/deletion, OIDC/multi-user roles, scheduled jobs and remote-source ingestion are absent. Definitions are structurally/budget checked, not validated against every FHIR profile. Broad native clinical gaps remain in the existing provider reports. The main frontend chunk is approximately 526 KiB uncompressed and Vite emits its standard chunk-size warning; route splitting can be a subsequent UI improvement. Public catalog queries still need database capacity for exact filtered counts. One configured worker, PostgreSQL statement-draining behavior and staging durability require the operational limits documented above. No new clinical composite or matched throughput superiority is claimed.
