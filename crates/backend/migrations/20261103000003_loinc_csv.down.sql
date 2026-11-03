DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM code_systems WHERE native_provider = 'loinc-csv') THEN
        RAISE EXCEPTION 'Cannot revert native LOINC while imported releases exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE loinc_terms;
DROP TABLE loinc_releases;
ALTER TABLE code_systems DROP CONSTRAINT code_systems_native_provider_check;
ALTER TABLE code_systems ADD CONSTRAINT code_systems_native_provider_check
    CHECK (native_provider IS NULL OR native_provider = 'snomed-rf2');
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
