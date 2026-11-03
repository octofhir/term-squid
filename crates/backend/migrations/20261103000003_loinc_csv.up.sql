-- The primary LOINC table is a native projection, not the complete Parts/Answers corpus.
ALTER TABLE code_systems DROP CONSTRAINT code_systems_native_provider_check;
ALTER TABLE code_systems ADD CONSTRAINT code_systems_native_provider_check
    CHECK (native_provider IS NULL OR native_provider IN ('snomed-rf2', 'loinc-csv'));

CREATE TABLE loinc_releases (
    code_system_id uuid PRIMARY KEY REFERENCES code_systems(id) ON DELETE CASCADE,
    version text NOT NULL,
    fingerprint text NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    provenance jsonb NOT NULL
);

-- Preserve every source column, including unimplemented accessory semantics and notices.
CREATE TABLE loinc_terms (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    code varchar(255) NOT NULL,
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, code)
);

CREATE TRIGGER loinc_release_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON loinc_releases
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER loinc_term_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON loinc_terms
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
