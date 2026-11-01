<p align="center"><img src=".github/banner.png" alt="octo/term-squid — A FHIR terminology server on Rust and PostgreSQL" width="100%"></p>

<p align="center"><b>A read-only FHIR terminology server for R4, R5 and R6, on Rust (Axum), PostgreSQL and a SolidJS UI.</b></p>

<p align="center">
  <a href="https://github.com/octofhir/term-squid/actions/workflows/ci.yml"><img src="https://github.com/octofhir/term-squid/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License: MIT OR Apache-2.0"></a>
</p>

## Status

✅ **Core Features Complete** - Backend API, CLI, and Frontend UI implemented

## Features

- ✅ **Multi-version FHIR Support** - R4, R5, and R6 endpoints
- ✅ **Read-only REST API** - Secure terminology server with GET-only operations
- ✅ **CLI Resource Creation** - Create CodeSystem, ValueSet, ConceptMap via command-line
- ✅ **FHIR Operations** - $lookup, $validate-code, $subsumes, $expand, $translate
- ✅ **PostgreSQL Storage** - Efficient storage with JSONB and full-text search
- ✅ **Package Loader CLI** - Import official FHIR packages from packages.fhir.org
- ✅ **Clinical Terminologies** - Easy ICD-10 and LOINC import
- ✅ **Modern Web UI** - SolidJS frontend with Linear-inspired design
- ✅ **Embedded Deployment** - Single binary with embedded frontend assets
- ✅ **Async-first Architecture** - High-performance async Rust with Tokio

## Quick Start

### Prerequisites

- Rust 1.75+
- Node.js 20+ with pnpm
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
cd frontend && pnpm install && cd ..

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
└── frontend/                     # SolidJS UI
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

## Implementation Progress

- ✅ Task 01: Project Setup
- ✅ Task 02: Storage Layer - PostgreSQL with JSONB and full-text search
- ✅ Task 03: FHIR REST API - Multi-version (R4/R5/R6) endpoints
- ✅ Task 04: FHIR Operations - $lookup, $validate-code, $subsumes, $expand, $translate
- ✅ Task 05: Package Loader CLI - Import from packages.fhir.org
- ✅ Task 06: Frontend Setup - SolidJS with embedded deployment
- ✅ Task 07: Frontend UI Components - Complete resource browsers with search
- 🚧 Task 08: Testing & Documentation (in progress)

## API Documentation

### Base URLs

- R4: `http://localhost:8081/api/r4`
- R5: `http://localhost:8081/api/r5`
- R6: `http://localhost:8081/api/r6`

### Security Model

**The REST API is read-only by design** - This ensures terminology integrity and prevents unauthorized modifications:

- ✅ **GET operations** - Search and read resources
- ❌ **POST/PUT/DELETE** - Not available via REST API
- ✅ **CLI operations** - Create resources via command-line tool with database access

This follows FHIR terminology server best practices where content is managed through controlled processes (CLI, package imports) rather than open REST APIs.

### REST Endpoints

#### CodeSystem

```bash
# Search CodeSystems
GET /api/r4/CodeSystem?name=international&status=active

# Get specific CodeSystem
GET /api/r4/CodeSystem/{id}
```

#### ValueSet

```bash
# Search ValueSets
GET /api/r4/ValueSet?name=administrative

# Get specific ValueSet
GET /api/r4/ValueSet/{id}
```

#### ConceptMap

```bash
# Search ConceptMaps
GET /api/r4/ConceptMap?name=map

# Get specific ConceptMap
GET /api/r4/ConceptMap/{id}
```

### FHIR Operations

#### $lookup - Find concept details

```bash
GET /api/r4/CodeSystem/$lookup?system=http://loinc.org&code=1234-5
```

Returns concept display, designation, and properties.

#### $validate-code - Validate code in ValueSet

```bash
GET /api/r4/ValueSet/$validate-code?url=http://hl7.org/fhir/ValueSet/example&code=test
```

Returns validation result with issues if invalid.

#### $subsumes - Test subsumption relationship

```bash
GET /api/r4/CodeSystem/$subsumes?system=http://snomed.info/sct&codeA=123&codeB=456
```

Returns: `equivalent`, `subsumes`, `subsumed-by`, or `not-subsumed`.

#### $expand - Expand ValueSet

```bash
GET /api/r4/ValueSet/$expand?url=http://hl7.org/fhir/ValueSet/example
```

Returns expanded ValueSet with all codes included.

#### $translate - Translate between code systems

```bash
GET /api/r4/ConceptMap/$translate?url=http://example.org/map&code=abc&system=http://example.org/source
```

Returns translated codes with equivalence relationships.

### Health and Stats

```bash
# Health check
GET /health

# Server statistics
GET /stats
```

## Architecture

### Technology Stack

**Backend:**
- Rust with Axum 0.8 - Modern async web framework
- PostgreSQL with SQLx - Type-safe database queries
- Tokio async runtime - High-performance async I/O
- rust-embed - Compile-time asset embedding

**Frontend:**
- SolidJS - Reactive UI framework
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
cd frontend && pnpm install
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
