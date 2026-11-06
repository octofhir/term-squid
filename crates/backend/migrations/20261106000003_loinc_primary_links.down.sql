-- Publication locks precede source absence checks; no loaded content is discarded.
LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE;
LOCK TABLE loinc_primary_links IN ACCESS EXCLUSIVE MODE;
LOCK TABLE loinc_releases IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM loinc_releases WHERE primary_links_loaded)
        OR EXISTS (SELECT 1 FROM loinc_primary_links)
    THEN
        RAISE EXCEPTION 'Cannot revert LOINC primary links while loaded cohorts or raw rows exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE loinc_primary_links;
ALTER TABLE loinc_releases
    DROP CONSTRAINT loinc_primary_link_source_availability,
    DROP COLUMN primary_link_source,
    DROP COLUMN primary_links_loaded;
DROP FUNCTION loinc_primary_link_source_valid(jsonb, text);
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
