-- Remove fhir_version column

DROP INDEX IF EXISTS idx_code_systems_fhir_version;
DROP INDEX IF EXISTS idx_value_sets_fhir_version;
DROP INDEX IF EXISTS idx_concept_maps_fhir_version;

ALTER TABLE code_systems DROP COLUMN IF EXISTS fhir_version;
ALTER TABLE value_sets DROP COLUMN IF EXISTS fhir_version;
ALTER TABLE concept_maps DROP COLUMN IF EXISTS fhir_version;
