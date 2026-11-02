-- Stored projections keep hot reads away from large TOASTed CodeSystem resources.
-- Generated columns also track CLI imports and direct content updates atomically.
ALTER TABLE code_systems
    ADD COLUMN hierarchy_meaning TEXT GENERATED ALWAYS AS (content->>'hierarchyMeaning') STORED,
    ADD COLUMN content_mode TEXT GENERATED ALWAYS AS (content->>'content') STORED,
    ADD COLUMN case_sensitive BOOLEAN GENERATED ALWAYS AS (COALESCE((content->>'caseSensitive')::boolean, true)) STORED;
CREATE INDEX idx_concepts_case_folded ON concepts(code_system_id, lower(code));
