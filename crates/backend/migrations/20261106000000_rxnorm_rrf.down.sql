DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM code_systems WHERE native_provider = 'rxnorm-rrf') THEN
        RAISE EXCEPTION 'Cannot revert native RxNorm while imported releases exist; back up and remove their CodeSystems first';
    END IF;
END $$;
DROP TABLE rxnorm_atoms;
DROP TABLE rxnorm_releases;
ALTER TABLE code_systems DROP CONSTRAINT code_systems_native_provider_check;
ALTER TABLE code_systems ADD CONSTRAINT code_systems_native_provider_check
    CHECK (native_provider IS NULL OR native_provider IN ('snomed-rf2', 'loinc-csv'));
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
