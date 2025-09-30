-- Add fhir_version column to track which FHIR version resources belong to

ALTER TABLE code_systems ADD COLUMN fhir_version VARCHAR(10);
ALTER TABLE value_sets ADD COLUMN fhir_version VARCHAR(10);
ALTER TABLE concept_maps ADD COLUMN fhir_version VARCHAR(10);

-- Extract FHIR version from content.fhirVersion if exists, otherwise set to 'R4' for existing data
UPDATE code_systems SET fhir_version = COALESCE(content->>'fhirVersion', 'R4') WHERE fhir_version IS NULL;
UPDATE value_sets SET fhir_version = COALESCE(content->>'fhirVersion', 'R4') WHERE fhir_version IS NULL;
UPDATE concept_maps SET fhir_version = COALESCE(content->>'fhirVersion', 'R4') WHERE fhir_version IS NULL;

-- Create indexes for filtering by FHIR version
CREATE INDEX idx_code_systems_fhir_version ON code_systems(fhir_version);
CREATE INDEX idx_value_sets_fhir_version ON value_sets(fhir_version);
CREATE INDEX idx_concept_maps_fhir_version ON concept_maps(fhir_version);
