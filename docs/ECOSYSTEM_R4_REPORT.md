# Pinned R4 terminology ecosystem gate

Historical preloaded checkpoint. The subsequent [request-context increment](REQUEST_CONTEXT_REPORT.md) supersedes the original unsupported `tx-resource` statements below: the original six cases now pass before any setup import, in both retention modes. The original evidence/profile below is retained separately.

November 4, 2026. This increment connects the official HL7 Java matcher to a reproducible selected R4 gate, fixes transport and Coding-response interoperability, and verifies prepared-data retention on and off. The complete `case` suite contains six tests. This is a selected preloaded-content profile, not approval as a complete HL7 terminology ecosystem server.

## Source and profile

[benchmarks/ecosystem/lock.json](../benchmarks/ecosystem/lock.json) pins:

- Published `hl7.fhir.uv.tx-ecosystem#1.9.6`, SHA-256 `704c80fae9a3b473a217fbdf751bb9ab29aa0a629b8e8221aad4af590ed32643`.
- Official Java validator **6.10.4**, SHA-256 `1106b9d58f9e363e47bea7c4fc065841e5fc91fe9d062775c3bfdd212bd653cc`.
- R4 **4.0.1**, mode `general`, the entire `case` suite, selected before examining pass/fail results.

The package's retained `tests/history.json` says 1.9.3 and explains that it is kept because TxTester reads its version there. Accordingly the official runner prints **tests v1.9.3** for this published 1.9.6 package. That source file is not rewritten. Archive checksums and per-file provenance identify the actual corpus; the banner is not substituted for package provenance or the server's FHIR version.

The official tests are authored in R5. The official Java runner discovers R4 from the server's real CapabilityStatement and performs its own request/response conversion and pattern matching. Its own R5→R4 converter prepares the four setup resources. The generated R4 package is imported through the existing CLI/shared transactional ingestion pipeline into a disposable standalone database.

The original runner sends suite setup as the `tx-resource` extension. term-squid does not implement that transient multi-resource protocol. The [test documentation](https://hl7.org/fhir/uv/tx-ecosystem/testcases.html) permits servers without on-the-fly resources to load setup another way. This gate therefore explicitly uses a **preloaded-content profile**:

1. Verify the immutable package and runner checksums.
2. Convert every setup resource using the official converter and import the resulting local package.
3. Generate a separate local manifest selecting the entire suite and removing only its already-loaded `setup` list.
4. Preserve every selected test, request, expected response, HTTP code, mode and pattern assertion. All non-manifest source bytes are checked before execution.
5. Run the unmodified official Java matcher against the real local endpoint. Require every expected test identity exactly once, six passes, zero failures/ignored tests and a successful runner exit code.

The downloaded package is preserved separately from the generated profile. Both the original inline setup mode and the preloaded mode remain executable with the same wrapper; no passing-case-derived filter or substituted comparator is used.

## Independent results and fixed behavior

The earlier delivered backend could not initially complete discovery: the official client appends a numeric `nocache` query value even to `/metadata`, and our strict extractor rejected it. After that transport fix, **0/6** `case` expectations passed because validation responses lacked Coding context and ecosystem diagnostics, despite already making the basic case-sensitive membership decisions.

The final selected profile passes **6/6** with retained plans enabled and **6/6** with retained plans disabled. Pinned local FHIRsmith 0.18.2, source `25a41f87711507789b361480ee4efae42df504d4`, passes **6/6** with the same converted setup and preloaded profile. Its request/provider code remains unmodified; the previously documented bootstrap-only R4 core package pin remains.

| Workflow | term-squid | Pinned local FHIRsmith |
| --- | --- | --- |
| Original suite setup sent as `tx-resource` | 0/6: explicit unsupported input | 6/6 |
| Complete suite with preloaded R4 setup | 6/6 | 6/6 |
| Preloaded suite with term-squid retention disabled | 6/6 | No equivalent reference retention toggle measured |

The original setup-mode gap remains visible in separate audit artifacts. It does not become a pass merely because preloaded tests pass. The server does not advertise transient `tx-resource` or `$cache-control` support.

Implemented changes:

- Recognize one bounded unsigned numeric `nocache` transport nonce on FHIR GET/POST query strings. It does not change terminology selection, disable generation checks or authorize anything. Duplicate, empty, invalid and oversized values fail. Unknown clinical parameters and POST query input restrictions remain errors.
- ValueSet validation of a Coding returns input `system`/`code`, the selected business version, canonical `normalized-code` where applicable, and structured `issues`. Case-insensitive spelling differences produce an informational `code-rule` issue; case-sensitive systems retain distinct codes.
- Distinguish a known code outside the selected set (`not-in-vs`) from a code missing in an unambiguously selected complete CodeSystem (`invalid-code`). A code known in another edition is not reported unknown solely because it is outside this set's edition. Versionless diagnostic resolution pins the actual NULL-version row by ID rather than treating NULL as “latest”.
- Positive Coding validation retains the existing prepared membership path. A missing member is re-evaluated with detailed context on one fresh repeatable-read snapshot; the initial negative plan result is discarded. No result combines the initial plan with later metadata. Multi-candidate unknown/indeterminate behavior remains intact.
- `TERMINOLOGY_PREPARED_CACHE=false` disables retention/reuse across concept, hierarchy, ValueSet and ConceptMap plans. Defaults remain true. SQL snapshots, generations, bounds, cancellation accounting and cross-process freshness remain. Normal public reads acquire no session/authorization queries. This is a diagnostic switch, not an alternative freshness policy.

New Rust regressions verify zero retained entries across all four cache families, immutable already-issued plans, updates observed by both modes, retained positive Coding preparation, selected-edition diagnostics and strict nonce/unknown-query handling. The previous assertions remain enabled.

## Toolchain, CI and reproduction

Use Python **3.12+**, Java **17+**, the documented Rust toolchain and Bun/k6. Local execution used Python 3.14, Java 21.0.2, Rust 1.99.0, Bun 1.4.3 and k6 2.3.0. No new Rust/npm dependency or database migration is required. Apply existing migrations through `20261104000006_supplements`. The React/Git UI-kit pins and frontend-before-RustEmbed build order remain.

```sh
python3 -B -m unittest discover -s benchmarks/ecosystem -p 'test_*.py'
python3 -B benchmarks/ecosystem.py prepare

# Use a disposable local database. This starts/stops its own backend per cache mode.
python3 -B benchmarks/ecosystem_smoke.py \
  --database-url 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_ecosystem' \
  --backend "$PWD/target/release/backend" --cli "$PWD/target/release/term-squid-cli"

# Against an independently started local reference loaded with the same setup:
python3 -B benchmarks/ecosystem.py run --url http://127.0.0.1:17004/tx/r4 \
  --output .benchmarks/UNIQUE-reference

# Retain the original setup-mode audit separately; an unsupported run exits nonzero.
python3 -B benchmarks/ecosystem.py run --setup inline --url http://127.0.0.1:18084/r4 \
  --output .benchmarks/UNIQUE-inline-audit
```

Prepared assets and provenance stay under `.benchmarks/ecosystem/`. Raw evidence stays under `.benchmarks/ecosystem-20261104/`. Run outputs must be new directories under `.benchmarks/`; all targets must be loopback URLs without embedded credentials. Preparation downloads only the pinned artifacts; test execution uses the verified local files without floating package resolution. Java settings permit HTTP/private network access only for the selected local FHIR base, retaining the validator's global network protection. Official package setup is CC0-1.0; the original package metadata/notices remain in the downloaded artifact.

CI adds a hard `ecosystem` gate with PostgreSQL, Java 17, Python 3.12, Bun and pinned k6 2.3.0. It consumes the built frontend artifact before compiling RustEmbed, imports real setup/unchanged synthetic fixtures and runs both retention modes. Expected/actual responses, the official TestReport and runner logs are preserved as artifacts. The benchmark harness job also runs the adapter's three tests. The existing embedded-browser job now explicitly installs k6, which its unchanged preflight command requires. No absolute performance test is added to shared CI.

## Delivery validation

The final backend artifact is `.benchmarks/ecosystem-20261104/backend-delivery`, SHA-256 `c78367a570e1bbb91ca4d052a43a918dc3a6778d852ba8fe2acb122bd0befcb2`. The CLI is unchanged from the merged delivery, SHA-256 `ff19e1af3f3ab5e285fa0bc308c1e4c39296c7ebe291c31599646271cb3e6182`. No generated jar, test corpus, credentials or raw responses are added to Git.

- **150 Rust/PostgreSQL tests, zero ignored**: 79 backend, 15 CLI, 46 core and 10 ingestion; `workspace-delivery.log`.
- Workspace/all-target/all-feature Clippy with warnings denied, formatting and whitespace checks pass; `clippy-delivery.log`. Locked offline workspace release build passes after the already-built unchanged frontend; `build-delivery.log`.
- **Three Python adapter tests** and **31 benchmark harness tests / 34,235 assertions** pass; `adapter-delivery.log` and `harness-delivery.log`. Existing semantic assertions/weights are unchanged.
- **6/6 official cases and 7/7 original local gates in each retention mode** on the final binary; `delivery-cache-parity/summary.json` and `delivery-cache-parity.log`. Counts come from the official/local result manifests. Reference preloaded and original setup audits remain separate.
- **23 embedded production browser/API/admin scenarios**, real JSON/package crash recovery and **7/7 unchanged local gates** pass on the final artifact; `production-delivery.log`. Desktop/mobile, light/dark, session/login/logout/expiry, import atomicity and rich R4 public reads remain covered.
- Frontend typecheck, lint and two unit tests pass. Frontend source/manifests/assets are unchanged in this increment. CI YAML parses and its frontend-first job dependency and explicit k6 setup are checked locally; the remote workflow has not been executed or pushed by this task.

The existing preview at `http://127.0.0.1:5173/` now uses the final backend on 8081 with the same standalone development database/session/bootstrap configuration. Temporary comparison/test processes are stopped. All implementation and prior comparison changes remain uncommitted; no push, publication or deployment was performed.

## Remaining work

Extend independent coverage to more complete suites, especially language, versions, fragments, composition and translation. Implement a bounded, isolated transient dependency context before claiming original `tx-resource` protocol support; do not silently ignore supplied resources or write public requests into persistent terminology tables. Full general-mode conformance, metadata ecosystem features, optional stateful extensions, clinical providers/corpus and the complete HealthSamurai cohort remain separate tasks. The [previous matched finite comparison](FHIRSMITH_API_COMPARISON.md) measures the earlier immutable backend; this interoperability increment makes no new throughput, clinical coverage or composite-score claim.
