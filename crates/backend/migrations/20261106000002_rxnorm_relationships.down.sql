-- Lock publication first, then raw facts and their source declaration, before absence checks.
LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE;
LOCK TABLE rxnorm_relationships IN ACCESS EXCLUSIVE MODE;
LOCK TABLE rxnorm_releases IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM rxnorm_releases WHERE relationships_loaded)
        OR EXISTS (SELECT 1 FROM rxnorm_relationships)
    THEN
        RAISE EXCEPTION 'Cannot revert RxNorm relationships while loaded cohorts or raw rows exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE rxnorm_relationships;
ALTER TABLE rxnorm_releases
    DROP CONSTRAINT rxnorm_relationship_source_availability,
    DROP COLUMN relationship_source,
    DROP COLUMN relationships_loaded;
DROP FUNCTION rxnorm_relationship_source_valid(jsonb, text);
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
