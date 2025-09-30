-- Create closure table for subsumption relationships

CREATE TABLE closure_table (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code_system_id UUID NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    ancestor_code VARCHAR(255) NOT NULL,
    descendant_code VARCHAR(255) NOT NULL,
    depth INTEGER NOT NULL CHECK (depth >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(code_system_id, ancestor_code, descendant_code)
);

CREATE INDEX idx_closure_ancestor ON closure_table(code_system_id, ancestor_code);
CREATE INDEX idx_closure_descendant ON closure_table(code_system_id, descendant_code);
CREATE INDEX idx_closure_depth ON closure_table(depth);
