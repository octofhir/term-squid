-- Stop publication before checking: a concurrent importer must not publish between
-- the absence check and table removal. Persistent publication starts at code_systems.
LOCK TABLE code_systems, rxnorm_atoms, rxnorm_semantic_types, rxnorm_attributes, rxnorm_releases
    IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM rxnorm_releases WHERE semantic_types_loaded OR attributes_loaded)
        OR EXISTS (SELECT 1 FROM rxnorm_semantic_types)
        OR EXISTS (SELECT 1 FROM rxnorm_attributes)
    THEN
        RAISE EXCEPTION 'Cannot revert RxNorm auxiliary sources while loaded cohorts or raw rows exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE rxnorm_attributes;
DROP TABLE rxnorm_semantic_types;
ALTER TABLE rxnorm_releases
    DROP CONSTRAINT rxnorm_auxiliary_source_availability,
    DROP COLUMN auxiliary_sources,
    DROP COLUMN attributes_loaded,
    DROP COLUMN semantic_types_loaded;
DROP FUNCTION rxnorm_auxiliary_source_valid(jsonb, text, text, bigint);
-- CONSO-only release identity, provenance, fingerprint and projections remain unchanged.
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
