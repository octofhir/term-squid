# FHIRsmith public UI and administration comparison

Inspected November 3, 2026. This is a capability/source comparison, not a throughput or clinical conformance result. No requests, writes or load were sent to public terminology servers.

The reference is the existing pinned local **FHIRsmith 0.18.2**, revision **`25a41f87711507789b361480ee4efae42df504d4`**. Its terminology README and router were downloaded again from that exact upstream revision; their SHA-256 values match the local reference copy:

- `tx/README.md`: `8e3dfc6fcf1f5a62f719b6259e4b61b7d214f8661bb389ab9ecfb2eb77eed6f0`.
- `tx/tx.js`: `22f17469c82c6bbd7d2e1d24d6e63c715397837cad063d2150e67aff9b41b055`.

| Workflow | FHIRsmith terminology module at this revision | term-squid increment |
|---|---|---|
| Public browsing | HTML resource/search pages, metadata, operation forms and ECL panel | Public React catalogs, version/detail/JSON pages and supported operation forms |
| Read operations over POST | Supported; POST is not inherently a mutation | Supported independently of administrator authentication |
| Install content | Operator supplies library YAML sources, local files/folders/packages and native importer output | Existing CLI plus authenticated single-resource FHIR JSON and bounded local R4 FHIR-package uploads |
| Change library configuration | Loaded once at startup; README explicitly requires restart after YAML changes | Successful import publishes metadata/projections and generation changes atomically without restarting readers |
| Resource writes through terminology HTTP routes | README declares unsupported writes; router rejects PUT/POST/DELETE/PATCH on resource instances with 405 | Mutation API is separately namespaced under `/api/admin`, with sessions, Origin and CSRF checks |
| Authenticated browser upload → inspection → persistent import job → activation | No equivalent workflow is registered in the inspected terminology router | Implemented for one R4 terminology JSON resource or an atomic bounded local R4 package; see [package scope](PACKAGE_IMPORT_REPORT.md) |
| Other administrative modules | Package registry and TestReport administration are separate modules | Standalone terminology administration; no external identity/database dependency |

FHIRsmith **does provide a public terminology UI and a substantial ingestion toolchain**. The inspected terminology module does **not** provide the same authenticated upload/job/online-activation workflow requested here. The absence finding is scoped to this exact module/revision; it is not a claim about every FHIRsmith module, deployment, future revision or terminology server. Its broader terminology/ECL/clinical capabilities are independent of this UI difference.

Primary sources: [terminology documentation](https://github.com/HealthIntersections/FHIRsmith/blob/25a41f87711507789b361480ee4efae42df504d4/tx/README.md), [terminology HTTP router](https://github.com/HealthIntersections/FHIRsmith/blob/25a41f87711507789b361480ee4efae42df504d4/tx/tx.js), [project modules and deployment documentation](https://github.com/HealthIntersections/FHIRsmith/blob/25a41f87711507789b361480ee4efae42df504d4/README.md).

Raw downloaded sources remain under `.benchmarks/ui-admin/fhirsmith/`. The pinned benchmark workloads, scores, weights and semantic assertions are unchanged.
