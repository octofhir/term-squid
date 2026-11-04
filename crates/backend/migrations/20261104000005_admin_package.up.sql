ALTER TABLE admin_import_jobs ADD COLUMN format TEXT NOT NULL DEFAULT 'fhir-json'
    CHECK (format IN ('fhir-json', 'fhir-package'));
