-- Native current-active RxNorm names retain the full attested monthly atom source.
ALTER TABLE code_systems DROP CONSTRAINT code_systems_native_provider_check;
ALTER TABLE code_systems ADD CONSTRAINT code_systems_native_provider_check
    CHECK (native_provider IS NULL OR native_provider IN ('snomed-rf2', 'loinc-csv', 'rxnorm-rrf'));

CREATE TABLE rxnorm_releases (
    code_system_id uuid PRIMARY KEY REFERENCES code_systems(id) ON DELETE CASCADE,
    version text NOT NULL,
    release_date date NOT NULL,
    fingerprint text NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    provenance jsonb NOT NULL
);

CREATE TABLE rxnorm_atoms (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    rxaui text NOT NULL,
    rxcui text NOT NULL,
    lat text NOT NULL,
    sab text NOT NULL,
    tty text NOT NULL,
    term text NOT NULL,
    suppress text NOT NULL CHECK (suppress IN ('N', 'O', 'Y', 'E')),
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, rxaui)
);
CREATE INDEX rxnorm_atom_concept ON rxnorm_atoms(code_system_id, rxcui);
CREATE INDEX rxnorm_atom_source ON rxnorm_atoms(code_system_id, sab, rxcui);
CREATE INDEX rxnorm_atom_type ON rxnorm_atoms(code_system_id, tty, rxcui) WHERE sab = 'RXNORM';

CREATE TRIGGER rxnorm_release_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_releases
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER rxnorm_atom_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_atoms
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
