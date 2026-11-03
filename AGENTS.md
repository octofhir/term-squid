# Agent Instructions

For a new session or another device, start with `docs/NEXT_SESSION.md`. Read `docs/DEVELOPMENT_HANDOFF.md`, `docs/PRODUCT_PLAN.md`, the relevant performance reports and `benchmarks/README.md` before continuing development.

- Communicate with the user in Russian. Keep Git-facing code, comments, documentation and reports in English.
- term-squid is a standalone terminology server with its own database and a reusable Rust library engine. Do not embed it into OctoFHIR or its database.
- Preserve existing working-tree changes. Commit, push or deploy only when the user requests it.
- Keep raw profiles, generated datasets, temporary notes and measurements under the gitignored `.benchmarks/` directory.
- Load-test only local instances, never public terminology servers.
- Preserve FHIR semantics, canonical/version resolution, update freshness and explicit unsupported-operation errors. Never weaken assertions to improve performance scores.
- Compare matched workloads against the pinned local FHIRsmith reference; improvements against our own baseline alone do not establish the product goal.
