-- Refuse to erase the format of package history or reinterpret staged archives as JSON.
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM admin_import_jobs WHERE format = 'fhir-package') THEN
        RAISE EXCEPTION 'Remove package jobs and their staging files explicitly before downgrading';
    END IF;
END $$;
ALTER TABLE admin_import_jobs DROP COLUMN format;
