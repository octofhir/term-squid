<p align="center"><img src=".github/banner.png" alt="octo/term-squid — A FHIR terminology server on Rust and PostgreSQL" width="100%"></p>

<p align="center"><b>A standalone FHIR terminology server under development, on Rust (Axum), PostgreSQL and a React UI.</b></p>

<p align="center">
  <a href="https://github.com/octofhir/term-squid/actions/workflows/ci.yml"><img src="https://github.com/octofhir/term-squid/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License: MIT OR Apache-2.0"></a>
</p>

## Status

The standalone R4 product API, branded public UI and authenticated administration are implemented. The six declared terminology operations cover all 70 pinned top-level R4 inputs, with explicit provider, registry and resource limits. See the [API implementation report](docs/API_IMPLEMENTATION_REPORT.md) and [requirements matrix](docs/API_REQUIREMENTS.md) for current evidence. R5/R6 aliases do not establish support for those releases.

The [latest R4 correctness/performance checkpoint](docs/SPEC_SPEED_REPORT.md) requires explicit ConceptMap equivalence, resolves dependency/product namespaces and returns versioned product `Coding` correctly. Fresh exact hierarchy hints, bounded queue reuse and a negotiated 1 KiB gzip threshold reduce temporary allocation traffic. Measured throughput still has regressions against the earlier baseline. Full R4 conformance, Unicode/default-case consistency and clinical composite leadership remain open.

See the [product development plan](docs/PRODUCT_PLAN.md) and [local benchmark instructions](benchmarks/README.md). Known correctness gaps are tracked there and exposed by semantic benchmark preflight.

## Features

- **FHIR routes** - R4, R5 and R6 route groups; release-specific conformance is pending
- **Read-only content API** - Resource search/read and GET/POST terminology operations; content management through CLI and protected administrator import jobs
- ✅ **CLI Resource Creation** - Create CodeSystem, ValueSet, ConceptMap via command-line
- **FHIR Operations** - $lookup, CodeSystem/ValueSet $validate-code, $subsumes, $expand and $translate, including typed inputs, dates/languages, bounded inline resources and version/context options
- ✅ **PostgreSQL Storage** - Efficient storage with JSONB and full-text search
- ✅ **Package Loader CLI** - Import official FHIR packages from packages.fhir.org
- **Native clinical imports** - Checksummed RF2 Snapshots, LOINC primary CSV and full-monthly RxNorm RRF with atomic activation; accessory/history limits remain explicit
- **Operational readiness** - Fresh bounded `/ready` storage/migration checks, separate from database-free `/health`
- **UCUM tools** - Grammar-backed conversion and canonical forms in the reusable library and database-free CLI
- ✅ **Modern Web UI** - React 19 with the actual OctoFHIR UI kit, public catalogs and protected administration
- ✅ **Embedded Deployment** - Single binary with embedded frontend assets
- ✅ **Async-first Architecture** - High-performance async Rust with Tokio

See [UI/admin setup, bootstrap, supported imports and validation](docs/UI_ADMIN_IMPLEMENTATION_REPORT.md). Administration ships without a default password.

## Quick Start

### Prerequisites

- Current stable Rust (locally verified with 1.99.0)
- Node.js 24.16.0 with pnpm 10.11.0 (pinned)
- PostgreSQL 16+
- Docker (optional but recommended)
- [just](https://github.com/casey/just) command runner (optional)

### Development Setup

1. **Clone the repository**
```bash
git clone https://github.com/octofhir/term-squid.git
cd term-squid
```

2. **Setup environment (with just)**
```bash
just setup
```

Or manually:
```bash
# Start PostgreSQL
docker-compose up -d postgres

# Install frontend dependencies
cd frontend && pnpm install --frozen-lockfile && pnpm run build && cd ..

# Build project
cargo build
```

3. **Run the server**
```bash
just run
# Or: cargo run --bin backend
```

The server will start on `http://localhost:8081` with the web UI embedded.

4. **Load FHIR data** (optional)
```bash
# Load core FHIR definitions
just load-defaults

# Load real clinical terminologies
just load-icd10  # Downloads ICD-10-CM from CDC
just load-loinc  # Requires free LOINC account

# Or load specific FHIR packages
just load-package hl7.fhir.r4.core 4.0.1
```

### Using Just Commands

We provide a comprehensive set of just commands for development:

```bash
# Setup and database
just setup              # Complete development setup
just db-start          # Start PostgreSQL
just db-stop           # Stop PostgreSQL
just db-reset          # Reset database (destroys data)

# Development
just run               # Run backend server
just run-debug         # Run with debug logging
just watch             # Run with hot reload (requires cargo-watch)
just dev               # Rebuild frontend and restart backend

# Frontend
just frontend-dev      # Run frontend dev server with HMR
just frontend-build    # Build frontend for production

# Testing and quality
just test              # Run all tests
just lint              # Run clippy
just fmt               # Format code
just check             # Run format check + lint + test

# Loading data
just load-defaults          # Load core FHIR packages (R4, R5, R6)
just load-icd10            # Download & import real ICD-10-CM from CDC
just load-loinc            # Download & import real LOINC (requires account)
just load-hl7-terminology  # Load HL7 FHIR terminology package
just load-package NAME VER # Load specific FHIR package

# Creating resources (via CLI)
just create-codesystem file.json   # Create CodeSystem from JSON
just create-valueset file.json     # Create ValueSet from JSON
just create-conceptmap file.json   # Create ConceptMap from JSON

# API testing
just health            # Check health endpoint
just stats             # Get server statistics
just search-cs "name"  # Search CodeSystems
just search-vs "name"  # Search ValueSets
just search-cm "name"  # Search ConceptMaps

# See all commands
just --list
```

## Project Structure

```
term-squid/
├── Cargo.toml                    # Workspace configuration
├── justfile                      # Just command runner recipes
├── docker-compose.yml            # PostgreSQL setup
├── .env                          # Environment configuration
├── crates/
│   ├── backend/                  # Axum API server
│   │   ├── Cargo.toml
│   │   ├── static/               # Embedded frontend assets (build output)
│   │   └── src/
│   │       ├── main.rs           # Server entry point with embedded assets
│   │       ├── config.rs         # Configuration management
│   │       ├── error.rs          # Error types
│   │       ├── api/              # REST API routes
│   │       │   ├── mod.rs
│   │       │   ├── codesystem.rs
│   │       │   ├── valueset.rs
│   │       │   ├── conceptmap.rs
│   │       │   └── operations.rs # FHIR operations
│   │       ├── store/            # Storage layer
│   │       │   ├── mod.rs
│   │       │   └── postgres.rs   # PostgreSQL implementation
│   │       └── models/           # Domain models
│   └── cli/                      # Package loader CLI
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
└── frontend/                     # React UI
    ├── package.json
    ├── vite.config.ts            # Configured to output to backend/static
    └── src/
        ├── App.tsx               # Main router
        ├── components/
        │   ├── Layout.tsx        # Main layout with sidebar
        │   └── common/           # Reusable UI components
        ├── pages/                # Route pages
        │   ├── Home.tsx          # Dashboard
        │   ├── CodeSystems.tsx   # CodeSystem browser
        │   ├── ValueSets.tsx     # ValueSet browser
        │   └── ConceptMaps.tsx   # ConceptMap browser
        └── styles/               # Global styles
            └── global.css        # Linear-inspired design system
```

## Benchmarks

Use the isolated local database and release server:

```sh
just bench-setup
just bench-server     # foreground; run benchmarks in another terminal
just bench-preflight
just bench --tests TS-CS01 --vus 1,10 --duration 5s
```

Requires Bun, k6, Docker Compose and sqlx-cli alongside Rust. The [benchmark guide](benchmarks/README.md) covers setup, the pinned upstream TX Benchmark, results and correctness gates. The [product plan](docs/PRODUCT_PLAN.md) defines milestones and acceptance criteria.

## API Documentation

### Base URLs

- R4: `http://localhost:8081/r4`
- R5: `http://localhost:8081/r5` (experimental R4 alias; no verified R5 adapter)
- R6: `http://localhost:8081/r6` (experimental R4 alias; no verified R6 adapter)

The current [R4 requirements matrix](docs/API_REQUIREMENTS.md) separates implemented inputs from unsupported semantics. Lookup and CodeSystem/ValueSet validation accept single POST Coding inputs with checked business versions and instance context; malformed requests return FHIR OperationOutcome. See [examples, limits and evidence](docs/API_CODING_REPORT.md). This does not establish full FHIR conformance.

### Security Model

Content is managed through CLI and protected `/admin` imports. The browser accepts one R4 terminology JSON resource or a bounded local R4 FHIR `.tgz` package with atomic all-resource activation; see [package support and limits](docs/PACKAGE_IMPORT_REPORT.md). Resource search/read uses GET. Terminology operations also accept POST Parameters requests; POST does not imply resource creation. Resource CRUD is not exposed.

### REST Endpoints

#### CodeSystem

```bash
# Search CodeSystems
GET /r4/CodeSystem?name=international&status=active

# Get specific CodeSystem
GET /r4/CodeSystem/{id}
```

#### ValueSet

```bash
# Search ValueSets
GET /r4/ValueSet?name=administrative

# Get specific ValueSet
GET /r4/ValueSet/{id}
```

#### ConceptMap

```bash
# Search ConceptMaps
GET /r4/ConceptMap?name=map

# Get specific ConceptMap
GET /r4/ConceptMap/{id}
```

### FHIR Operations

#### $lookup - Find concept details

```bash
GET /r4/CodeSystem/$lookup?system=http://loinc.org&code=1234-5
```

Returns concept display, designation, and properties.

#### $validate-code - Validate code in ValueSet

```bash
GET /r4/ValueSet/$validate-code?url=http://hl7.org/fhir/ValueSet/example&code=test
```

Returns validation result with issues if invalid.

#### $subsumes - Test subsumption relationship

```bash
GET /r4/CodeSystem/$subsumes?system=http://snomed.info/sct&codeA=123&codeB=456
```

Returns: `equivalent`, `subsumes`, `subsumed-by`, or `not-subsumed`.

#### $expand - Expand ValueSet

```bash
GET /r4/ValueSet/$expand?url=http://hl7.org/fhir/ValueSet/example
```

Returns expanded ValueSet with all codes included.

#### $translate - Translate between code systems

```bash
GET /r4/ConceptMap/$translate?url=http://example.org/map&code=abc&system=http://example.org/source
```

Returns translated codes with equivalence relationships.

### Health and Stats

```bash
# Health check
GET /health

# Fresh schema/database readiness (200 or 503; not clinical coverage)
GET /ready

# Server statistics
GET /stats
```

Readiness requires all compiled migrations, 19 enabled generation-tracking triggers and consistent native release registrations, including optional source descriptors and the bounded SNOMED refset registry. Limits: two seconds overall, one-second SQL statements and four probes. A loaded clinical corpus and administration tables are not required. Apply migrations through `20261106000003_loinc_primary_links` and `20261106000004_snomed_simple_refsets` before the updated backend. See [the readiness report](docs/RXNORM_READINESS_REPORT.md) and [the primary-link/refset report](docs/LOINC_PARTS_SNOMED_REFSETS_REPORT.md) for scope and upgrades.

### Bounded SNOMED ECL

Native RF2 ValueSets accept `constraint = [ECL]` and implicit `http://snomed.info/sct?fhir_vs=ecl/[percent-encoded ECL]` URLs, including selected edition/version contexts. Encode the ECL segment once; form-style `+` is not a space. The subset covers active precoordinated concepts, `*`, `<`, `<<`, `>`, `>>`, `<!`, `<<!`, `>!`, `>>!`, and `AND`/`OR`/`MINUS` with parentheses. Mixed operators and repeated `MINUS` require explicit grouping; hierarchy operators take scalar focuses, not sets or wildcards. Labels are ignored descriptions, not validated displays.

Only active inferred is-a paths participate, including active intermediate concepts. Unknown or inactive focuses fail explicitly. Limits include 4 KiB of ECL, 32 grouping levels, 128 expression nodes and sixteen hierarchy walks shared with other native filters. Scalar/hierarchy ECL uses existing RF2 projections without an additional source. Refinements, postcoordination and historical associations remain unsupported. See [the ECL/relationship report](docs/ECL_RXNORM_RELATIONS_REPORT.md).

Optional `simpleRefsets` in the RF2 manifest adds declared concept-only Simple refsets. Its descriptor supplies `path`, lowercase `sha256`, `expectedRows` (including zero), the same `release` as the base Snapshot, `completeSnapshot: true`, `conceptOnly: true`, and 1–64 lexicographically sorted unique `refsetIds`. Declare even an empty refset explicitly. The file must be a same-release `SimpleSnapshot` with the six standard columns; Full/Delta files and nonconcept membership are rejected. Each declared refset must be an active concept with a bounded active inferred path to Simple type reference set `446609009`; attestation alone is not type proof.

Loaded declarations support `concept in [refset-id]`, implicit `http://snomed.info/sct?fhir_vs=refset/[refset-id]` URLs and scalar ECL `^ [refset-id]`, with normal edition/version selection. Missing or undeclared sources return 422, including empty queries and early membership matches. An explicitly loaded empty refset gives an empty result. Ordinary refset selection follows active member rows, which may reference inactive concepts; ECL still restricts results to active concepts. Global refset enumeration, nonconcept/map refsets, selector or field projections, refinements and historical associations remain unsupported. Select the source cohort on first publication: absent/null preserves legacy identity, while later enrichment of an immutable edition conflicts. See [the primary-link/refset report](docs/LOINC_PARTS_SNOMED_REFSETS_REPORT.md).

### LOINC coded primary axes

The native `import-loinc --manifest [path]` route accepts optional `primaryLinks` alongside `Loinc.csv`. The descriptor supplies `path` to `LoincPartLink_Primary.csv`, lowercase `sha256`, `expectedRows`, the same `version` as the main table and `completeFile: true`. Import the complete same-edition cohort on first publication; adding a file to an immutable edition later is a conflict. Omitting it, or using null, preserves the old main-table fingerprint. Native clinical imports remain CLI-only.

Loaded primary links expose `COMPONENT`, `PROPERTY`, `TIME_ASPCT`, `SYSTEM`, `SCALE_TYP` and `METHOD_TYP` as FHIR `code` properties with `valueCode: LP…`. The existing `raw-*` properties retain verbatim `valueString` labels. Five primary axes are required for every main-table term, plus Method exactly when specified; URI/type and source labels must agree. All source rows, including Radiology/DocumentOntology and foreign-system links, remain raw, but only eligible Primary links supply these six coded properties.

The six axes support `=`; bounded `in` (at most 256 LP literals) and `exists` are explicit server extensions. LP prefixes compare ASCII-case-insensitively and are returned in canonical uppercase. A valid unknown LP literal gives no matching primary terms; it is not a Part-existence decision. Expansion still requires explicit primary concepts or `source-table=Loinc.csv`; an axis filter does not establish that scope. Missing source returns 422 for coded filters or an explicit coded-property lookup, even for `count=0`. With a loaded file, an unspecified Method is simply absent, and `METHOD_TYP exists false` can select it. Default/`*` lookup returns available properties without requiring the optional file.

This does not load a Part registry or enable Part lookup, accessory/all-code enumeration, regex filters, hierarchy or implicit `/vs/LP…` sets. See [the primary-link/refset report](docs/LOINC_PARTS_SNOMED_REFSETS_REPORT.md).

### RxNorm import and UCUM tools

```sh
term-squid-cli --database-url "$DATABASE_URL" migrate
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json --dry-run
term-squid-cli --database-url "$DATABASE_URL" import-rxnorm --manifest .benchmarks/clinical/rxnorm/manifest.json
# These commands need no database:
term-squid-cli ucum-convert --value -40 --from Cel --to '[degF]'
term-squid-cli ucum-canonical --code km
```

Copy/fill the [RxNorm template](benchmarks/rxnorm-rrf.template.json) for an independently obtained full monthly `RXNCONSO.RRF`. Known current-active normalized codes, aliases, source/TTY filters and scoped mixed ValueSets are supported; absent historical knowledge fails explicitly. The template intentionally fails unchanged; generated fixtures are not clinical releases.

Optional `semanticTypes` (`RXNSTY.RRF`) and `attributes` (`RXNSAT.RRF`) manifest descriptors add semantic types and selected NDC attributes. Each descriptor supplies `path`, lowercase `sha256`, `expectedRows`, the same `version` as CONSO and `fullMonthly: true`. These sources require migration `20261106000001_rxnorm_auxiliary`. Include the complete source cohort on the edition's first import: adding files to an already published edition is an immutable conflict, not an in-place enrichment. Native imports remain CLI-only.

The `STY` filter supports `=` and `in`, defaults to TUI (`T200` or `TUI:T200`), and accepts explicit `STN:A1.3.3` or `STY:Clinical Drug` qualifiers. One list cannot mix qualifier families. The custom `NDC` property/filter (`urn:term-squid:rxnorm:NDC`) uses exact current NLM-curated 11-digit values for supported active product atoms; leading zeros are preserved. No padding, formatting conversion or inferred drug mapping is performed.

Explicit lookup requests for `TUI`/`STN`/`STY` or `NDC`, and their filters, return 422 when the required file is unavailable. A loaded file with no value for a known active concept means known absence, not an unavailable source. Semantic/NDC files alone do not provide relationship or historical knowledge. See [the auxiliary-source report](docs/RXNORM_AUXILIARY_REPORT.md) for scope, examples and limits.

Optional `relationships` imports `RXNREL.RRF` with the same descriptor fields and same-release/full-monthly requirements. Select it on first publication; it cannot be appended to an immutable edition later. Apply all migrations through `20261106000002_rxnorm_relationships` before the updated backend.

Relationship filters support the seven declared `REL` codes and 28 `RELA` names as properties, with `=`/`in` and explicit `CUI:` or `AUI:` targets. They select candidate endpoint 1 anchored at endpoint 2: the source describes endpoint 2's relation **to** endpoint 1, not intuitive forward adjacency. Foreign-SAB facts can match; only rows with empty or `N` suppression participate. A missing file returns 422, while a loaded file with no matching fact gives a normal empty result. This does not add RxNorm history, subsumption, UNII, automatic maps or another FHIR operation. See [the ECL/relationship report](docs/ECL_RXNORM_RELATIONS_REPORT.md) for the exact supported names and bounds.

## Architecture

### Technology Stack

**Backend:**
- Rust with Axum 0.8 - Modern async web framework
- PostgreSQL with SQLx - Type-safe database queries
- Tokio async runtime - High-performance async I/O
- rust-embed - Compile-time asset embedding

**Frontend:**
- React 19 - Public browsing and standalone administration with the actual OctoFHIR UI kit
- TypeScript - Type-safe JavaScript
- Vite - Fast build tool
- CSS Modules - Scoped styling
- Linear design system - Modern, minimal UI

**CLI:**
- Rust with clap - Command-line argument parsing
- reqwest - HTTP client for package downloads

### Key Design Decisions

1. **Embedded Deployment** - Frontend assets compiled into Rust binary for single-file deployment
2. **Multi-version Support** - Separate endpoints for FHIR R4, R5, and R6
3. **JSONB Storage** - PostgreSQL JSONB for flexible schema with SQL query power
4. **Full-text Search** - PostgreSQL tsvector for efficient text search
5. **Async-first** - Tokio for high-concurrency request handling

## Deployment

### Production Build

```bash
# Build frontend and backend in release mode
just build-release

# The binary will be at: target/release/backend
# It contains all frontend assets embedded
```

### Running in Production

```bash
# Set environment variables
export DATABASE_URL="postgresql://user:pass@host:5432/term_squid"
export RUST_LOG=info

# Run the server
./target/release/backend
```

### Docker Deployment (Optional)

```bash
# Build Docker image
just docker-build

# Run container
just docker-run
```

### Environment Variables

- `DATABASE_URL` - PostgreSQL connection string (default: `postgresql://termserver:dev_password@localhost:5433/termserver`)
- `DATABASE_MAX_CONNECTIONS` - SQLx pool capacity per backend process, 1–64 (default: `32`). Explicit settings remain supported; budget all replicas, CLI imports and administrative sessions against the database connection limit.
- `SERVER_HOST` - Server host (default: `127.0.0.1`)
- `SERVER_PORT` - Server port (default: `8081`)
- `RUST_LOG` - Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`)

## Development Workflow

### Typical Development Session

```bash
# Start database
just db-start

# Run backend with hot reload (in one terminal)
just watch

# Run frontend dev server with HMR (in another terminal)
just frontend-dev

# Access:
# - Backend: http://localhost:8081
# - Frontend dev: http://localhost:5173 (proxies API to backend)
```

### Before Committing

```bash
# Run all checks
just check

# This runs:
# - cargo fmt --check
# - cargo clippy
# - cargo test
```

## CLI Usage

### Package Management

```bash
# Import FHIR packages
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL import hl7.fhir.r4.core --version 4.0.1 -y

# Import default packages (R4, R5, R6 core)
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL import-defaults --version all -y

# List installed packages
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL list

# Show statistics
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL stats
```

### Creating Resources

Create individual FHIR resources from JSON files:

```bash
# Create a CodeSystem
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL create-code-system my-codesystem.json

# Create a ValueSet
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL create-value-set my-valueset.json

# Create a ConceptMap
cargo run --bin term-squid-cli -- --database-url $DATABASE_URL create-concept-map my-conceptmap.json
```

Example FHIR JSON file structure:

```json
{
  "resourceType": "CodeSystem",
  "url": "http://example.org/my-codesystem",
  "version": "1.0.0",
  "name": "MyCodeSystem",
  "title": "My Custom Code System",
  "status": "active",
  "content": "complete",
  "concept": [
    {
      "code": "code1",
      "display": "Code 1",
      "definition": "First code in the system"
    }
  ]
}
```

### Loading Clinical Terminologies

**Important**: The CLI automatically imports CodeSystems with their concepts, ValueSets, and ConceptMaps. You don't need separate commands for each resource type.

#### HL7 Terminology Package

The HL7 terminology package contains FHIR infrastructure resources:

```bash
just load-hl7-terminology

# This includes:
# - FHIR ValueSets and CodeSystems (definitions only, not actual codes)
# - ICD-10/ICD-10-CM definitions (structure, not diagnosis codes)
# - LOINC definitions (structure, not lab codes)
# - SNOMED CT, RxNorm, UCUM references
# - ConceptMaps for terminology mappings
```

**Note**: This package does NOT include actual ICD-10 or LOINC codes. They are too large for FHIR packages and have licensing restrictions.

#### ICD-10-CM (Actual Diagnosis Codes)

For real diagnosis codes from CDC:

```bash
just load-icd10

# Downloads and imports:
# - CodeSystem resource with metadata
# - ~70,000 diagnosis code concepts
# - Automatically parsed from CDC's official XML
# Source: https://ftp.cdc.gov/pub/Health_Statistics/NCHS/Publications/ICD10CM/2026/
```

#### LOINC (Actual Lab/Observation Codes)

For real LOINC codes (requires manual download due to licensing):

```bash
# Step 1: Get LOINC (free but requires account)
# Visit: https://loinc.org/downloads/
# Download: LOINC Table File (CSV)
# Place Loinc.csv in: data/loinc/

# Step 2: Import
just load-loinc

# Imports:
# - CodeSystem resource with metadata
# - ~90,000 lab/observation code concepts
# - Merged from official LOINC CSV
```

## Troubleshooting

### Database Connection Issues

```bash
# Check PostgreSQL is running
just db-check

# Reset database if corrupted
just db-reset
```

### Frontend Build Issues

```bash
# Clean and rebuild
just clean
just frontend-install
just build
```

### Port Already in Use

```bash
# Check what's using port 8081
lsof -i :8081

# Or change port in .env
echo "SERVER_PORT=8082" >> .env
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
