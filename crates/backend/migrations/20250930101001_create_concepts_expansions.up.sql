-- Create concepts and expansions tables

CREATE TABLE concepts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code_system_id UUID NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    code VARCHAR(255) NOT NULL,
    display TEXT,
    definition TEXT,
    properties JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(code_system_id, code)
);

CREATE INDEX idx_concepts_code_system ON concepts(code_system_id);
CREATE INDEX idx_concepts_code ON concepts(code);
CREATE INDEX idx_concepts_display ON concepts USING gin(to_tsvector('english', COALESCE(display, '')));

CREATE TABLE value_set_expansions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    value_set_id UUID NOT NULL REFERENCES value_sets(id) ON DELETE CASCADE,
    expansion_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_expansions_value_set ON value_set_expansions(value_set_id);
CREATE INDEX idx_expansions_created_at ON value_set_expansions(created_at DESC);
