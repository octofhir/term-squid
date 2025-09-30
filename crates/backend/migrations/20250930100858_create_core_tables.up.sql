-- Create core FHIR terminology tables

CREATE TABLE code_systems (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url VARCHAR(512) NOT NULL,
    version VARCHAR(100),
    status VARCHAR(20) NOT NULL CHECK (status IN ('draft', 'active', 'retired', 'unknown')),
    name VARCHAR(255),
    title TEXT,
    content JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(url, version)
);

CREATE INDEX idx_code_systems_url ON code_systems(url);
CREATE INDEX idx_code_systems_status ON code_systems(status);
CREATE INDEX idx_code_systems_name ON code_systems(name);
CREATE INDEX idx_code_systems_title ON code_systems USING gin(to_tsvector('english', COALESCE(title, '')));

CREATE TABLE value_sets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url VARCHAR(512) NOT NULL,
    version VARCHAR(100),
    status VARCHAR(20) NOT NULL CHECK (status IN ('draft', 'active', 'retired', 'unknown')),
    name VARCHAR(255),
    title TEXT,
    content JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(url, version)
);

CREATE INDEX idx_value_sets_url ON value_sets(url);
CREATE INDEX idx_value_sets_status ON value_sets(status);
CREATE INDEX idx_value_sets_name ON value_sets(name);
CREATE INDEX idx_value_sets_title ON value_sets USING gin(to_tsvector('english', COALESCE(title, '')));

CREATE TABLE concept_maps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url VARCHAR(512) NOT NULL,
    version VARCHAR(100),
    status VARCHAR(20) NOT NULL CHECK (status IN ('draft', 'active', 'retired', 'unknown')),
    name VARCHAR(255),
    title TEXT,
    source_uri VARCHAR(512),
    target_uri VARCHAR(512),
    content JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(url, version)
);

CREATE INDEX idx_concept_maps_url ON concept_maps(url);
CREATE INDEX idx_concept_maps_status ON concept_maps(status);
CREATE INDEX idx_concept_maps_name ON concept_maps(name);
CREATE INDEX idx_concept_maps_source ON concept_maps(source_uri);
CREATE INDEX idx_concept_maps_target ON concept_maps(target_uri);
