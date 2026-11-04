# term-squid - FHIR Terminology Server
# Just commands for common development tasks

bench_database_url := env('BENCH_DATABASE_URL', 'postgresql://benchmark:benchmark@127.0.0.1:15433/term_squid_benchmark')

# List all available commands
default:
    @just --list

# Setup the development environment
setup:
    @echo "Setting up development environment..."
    docker-compose up -d postgres
    cd frontend && pnpm install --frozen-lockfile
    cd frontend && pnpm run build
    cargo build --locked
    @echo "✓ Setup complete!"

# Start PostgreSQL database
db-start:
    docker-compose up -d postgres

# Stop PostgreSQL database
db-stop:
    docker-compose stop postgres

# Reset database (WARNING: destroys all data)
db-reset:
    docker-compose down -v postgres
    docker-compose up -d postgres
    sleep 2
    cargo run --bin backend

# Run database migrations
db-migrate:
    sqlx migrate run

# Check database connection
db-check:
    psql "postgresql://termserver:dev_password@localhost:5433/termserver" -c "SELECT version();"

# Build the entire project
build: frontend-build
    cargo build --locked

# Build in release mode
build-release: frontend-build
    cargo build --locked --release

# Run the backend server
run:
    cargo run --bin backend

# Run the backend server with debug logging
run-debug:
    RUST_LOG=debug cargo run --bin backend

# Run the backend server in watch mode (requires cargo-watch)
watch:
    cargo watch -x 'run --bin backend'

# Run all tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without making changes
fmt-check:
    cargo fmt --check

# Run all checks (format, lint, test)
check: fmt-check lint test

# Clean build artifacts
clean:
    cargo clean
    rm -rf frontend/node_modules
    rm -rf crates/backend/static

# Load default FHIR packages (R4, R5, R6 core)
load-defaults:
    @echo "Loading default FHIR packages..."
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" import-defaults --version all -y
    @echo "✅ Core packages loaded!"

# Load HL7 terminology package (includes CodeSystems, ValueSets, and ConceptMaps)
# This package contains FHIR infrastructure terminologies and references to external systems
load-hl7-terminology:
    @echo "Loading HL7 FHIR terminology package..."
    @echo "This includes:"
    @echo "  • FHIR value sets and code systems"
    @echo "  • ICD-10/ICD-10-CM definitions (not codes)"
    @echo "  • LOINC definitions (not codes)"
    @echo "  • SNOMED CT, RxNorm, UCUM references"
    @echo "  • ConceptMaps and terminology mappings"
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" import hl7.terminology.r4 --version 6.5.0 -y
    @echo "✅ HL7 terminology package loaded!"
    @echo ""
    @echo "ℹ️  Note: Full ICD-10 and LOINC codes are NOT included"
    @echo "   They are too large for FHIR packages and have licensing restrictions"
    @echo "   Use ./scripts/download-icd10cm.sh and ./scripts/download-loinc.sh for actual codes"

# Download ICD-10-CM codes from CDC and convert to FHIR CodeSystem
# This gets the ACTUAL diagnosis codes (~70,000 codes)
load-icd10:
    @echo "📥 Downloading ICD-10-CM from CDC..."
    @echo "Source: https://ftp.cdc.gov/pub/Health_Statistics/NCHS/Publications/ICD10CM/2026/"
    ./scripts/download-icd10cm.sh
    @echo ""
    @echo "📤 Importing CodeSystem with concepts into database..."
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" create-code-system data/icd10cm/icd10cm-codesystem.json
    @echo "✅ ICD-10-CM loaded with all diagnosis codes!"

# Download LOINC codes and convert to FHIR CodeSystem
# This gets the ACTUAL lab/observation codes (~90,000 codes)
# Requires manual download due to LOINC licensing
load-loinc:
    @echo "📥 Setting up LOINC..."
    @echo ""
    @echo "⚠️  LOINC requires manual download (free but requires acceptance of terms)"
    @echo "Steps:"
    @echo "  1. Create free account at https://loinc.org"
    @echo "  2. Download 'LOINC Table File (CSV)'"
    @echo "  3. Extract and place Loinc.csv in data/loinc/"
    @echo "  4. Run this command again"
    @echo ""
    ./scripts/download-loinc.sh
    @echo ""
    @echo "📤 Importing CodeSystem with concepts into database..."
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" create-code-system data/loinc/loinc-codesystem.json
    @echo "✅ LOINC loaded with all observation codes!"

# Load a FHIR package (example: just load-package hl7.fhir.r4.core 4.0.1)
load-package package version:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" import {{package}} --version {{version}} -y

# Show package statistics
cli-stats:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" stats

# List installed packages
cli-list:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" list

# Create a CodeSystem from a FHIR JSON file
# Example: just create-codesystem ./my-codesystem.json
create-codesystem file:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" create-code-system {{file}}

# Create a ValueSet from a FHIR JSON file
# Example: just create-valueset ./my-valueset.json
create-valueset file:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" create-value-set {{file}}

# Create a ConceptMap from a FHIR JSON file
# Example: just create-conceptmap ./my-conceptmap.json
create-conceptmap file:
    cargo run --bin term-squid-cli -- --database-url "postgresql://termserver:dev_password@localhost:5433/termserver" create-concept-map {{file}}

# Check health endpoint
health:
    curl -s http://localhost:8081/health | jq

# Get server statistics
stats:
    curl -s http://localhost:8081/stats | jq

# Search CodeSystems (example: just search-cs "international")
search-cs query:
    curl -s "http://localhost:8081/api/r4/CodeSystem?name={{query}}" | jq

# Search ValueSets (example: just search-vs "administrative")
search-vs query:
    curl -s "http://localhost:8081/api/r4/ValueSet?name={{query}}" | jq

# Search ConceptMaps (example: just search-cm "map")
search-cm query:
    curl -s "http://localhost:8081/api/r4/ConceptMap?name={{query}}" | jq

# Get a specific CodeSystem by ID
get-cs id:
    curl -s "http://localhost:8081/api/r4/CodeSystem/{{id}}" | jq

# Get a specific ValueSet by ID
get-vs id:
    curl -s "http://localhost:8081/api/r4/ValueSet/{{id}}" | jq

# Get a specific ConceptMap by ID
get-cm id:
    curl -s "http://localhost:8081/api/r4/ConceptMap/{{id}}" | jq

# Test $lookup operation
lookup system code:
    curl -s "http://localhost:8081/api/r4/CodeSystem/\$lookup?system={{system}}&code={{code}}" | jq

# Test $validate-code operation
validate-code url code:
    curl -s "http://localhost:8081/api/r4/ValueSet/\$validate-code?url={{url}}&code={{code}}" | jq

# Test $expand operation
expand url:
    curl -s "http://localhost:8081/api/r4/ValueSet/\$expand?url={{url}}" | jq

# Development workflow: rebuild frontend and restart backend
dev:
    cd frontend && pnpm run build
    cargo run --bin backend

# Frontend: install dependencies
frontend-install:
    cd frontend && pnpm install --frozen-lockfile

# Frontend: run dev server with HMR
frontend-dev:
    cd frontend && pnpm run dev

# Frontend: build for production
frontend-build:
    cd frontend && pnpm run build

# Frontend: preview production build
frontend-preview:
    cd frontend && pnpm run preview

# Docker: build production image
docker-build:
    docker build -t term-squid:latest .

# Docker: run production container
docker-run:
    docker run -p 8081:8081 --env-file .env term-squid:latest

# Show logs from PostgreSQL
logs-db:
    docker-compose logs -f postgres

# Open the web UI in browser (macOS)
open:
    open http://localhost:8081

# Benchmark the terminology API against generated local fixtures (requires Bun and k6)
[positional-arguments]
bench *args:
    bun benchmarks/bench.ts run "$@"

# Check terminology responses without applying load
[positional-arguments]
bench-preflight *args:
    bun benchmarks/bench.ts run --preflight-only "$@"

# Download the pinned HealthSamurai TX Benchmark
bench-fetch:
    bun benchmarks/bench.ts fetch

# Run the original TX Benchmark scenarios without the observability stack
[positional-arguments]
bench-upstream *args:
    bun benchmarks/bench.ts run --suite upstream "$@"

# Generate a repeatable synthetic FHIR package
bench-fixtures:
    bun benchmarks/bench.ts fixtures

# Import the synthetic package into the benchmark database through the existing CLI
bench-seed:
    DATABASE_URL="{{bench_database_url}}" CARGO_TARGET_DIR=.benchmarks/target bun benchmarks/bench.ts seed

# Start an isolated benchmark database
bench-db:
    docker compose -f benchmarks/compose.yaml up -d --wait

# Apply the actual application migrations to the benchmark database
bench-migrate:
    CARGO_TARGET_DIR=.benchmarks/target cargo run --release --bin term-squid-cli -- --database-url "{{bench_database_url}}" migrate

# Prepare the isolated database and synthetic data
bench-setup: bench-db bench-migrate bench-seed

# Run the release server in the foreground on the benchmark port
bench-server: frontend-install frontend-build
    DATABASE_URL="{{bench_database_url}}" SERVER_HOST=127.0.0.1 SERVER_PORT=18081 RUST_LOG=warn CARGO_TARGET_DIR=.benchmarks/target cargo run --release --bin backend

# Stop benchmark containers while preserving their data
bench-stop:
    docker compose -f benchmarks/compose.yaml down

# Check benchmark orchestration and semantic assertions
bench-test:
    bun test benchmarks

# Benchmark the health endpoint (transport smoke test only; requires Apache Bench)
bench-health:
    ab -n 1000 -c 10 http://localhost:8081/health

# Inspect the pinned HealthSamurai Round 0 archive without contacting its production server
bench-reference:
    bun benchmarks/healthsamurai.ts reference

# Canonical upstream timing; require full preflight coverage before any local load
[positional-arguments]
bench-round0 *args:
    bun benchmarks/bench.ts run --suite upstream --strict "$@"

# Original HealthSamurai UI plus a separate page for our matched local measurements
bench-ui:
    bun benchmarks/ui.ts serve

# Build the generated local benchmark report site without starting a server
bench-ui-build:
    bun benchmarks/ui.ts build

# Score a complete, same-environment term-squid / Termbox / FHIRsmith upstream cohort
bench-score run:
    bun benchmarks/healthsamurai.ts score {{run}}
