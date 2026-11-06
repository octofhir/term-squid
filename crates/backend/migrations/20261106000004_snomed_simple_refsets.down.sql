-- Serialize publication before checking both declarations and raw facts.
LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE;
LOCK TABLE snomed_simple_members IN ACCESS EXCLUSIVE MODE;
LOCK TABLE snomed_simple_refsets IN ACCESS EXCLUSIVE MODE;
LOCK TABLE snomed_releases IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM snomed_releases WHERE simple_refsets_loaded)
        OR EXISTS (SELECT 1 FROM snomed_simple_refsets)
        OR EXISTS (SELECT 1 FROM snomed_simple_members)
    THEN
        RAISE EXCEPTION 'Cannot revert SNOMED Simple refsets while loaded cohorts, registry or raw rows exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE snomed_simple_members;
DROP TABLE snomed_simple_refsets;
ALTER TABLE snomed_releases
    DROP CONSTRAINT snomed_simple_refset_source_availability,
    DROP COLUMN simple_refset_source,
    DROP COLUMN simple_refsets_loaded;
DROP FUNCTION snomed_simple_refset_source_valid(jsonb, text);
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
