-- Downgrading must not leave native concepts advertised as an empty finite hierarchy.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM code_systems WHERE native_provider IS NOT NULL) THEN
        RAISE EXCEPTION 'Cannot revert native RF2 while imported releases exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE snomed_language_members;
DROP TABLE snomed_relationships;
DROP TABLE snomed_descriptions;
DROP TABLE snomed_releases;
ALTER TABLE code_systems DROP COLUMN native_provider;
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
