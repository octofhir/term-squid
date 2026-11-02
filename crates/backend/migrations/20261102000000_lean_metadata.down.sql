DROP INDEX idx_concepts_case_folded;
ALTER TABLE code_systems DROP COLUMN hierarchy_meaning, DROP COLUMN content_mode, DROP COLUMN case_sensitive;
