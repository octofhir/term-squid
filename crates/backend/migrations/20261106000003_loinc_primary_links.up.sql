-- A complete same-edition primary-link source is independent of the main-table scope.
CREATE FUNCTION loinc_primary_link_source_valid(resource jsonb, expected_version text)
RETURNS boolean LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT COALESCE(
        jsonb_typeof(resource) = 'object'
        AND octet_length(resource::text) <= 4096
        AND resource ?& ARRAY['file','sha256','rows','bytes','version','completeFile']
        AND resource - 'file' - 'sha256' - 'rows' - 'bytes' - 'version' - 'completeFile' = '{}'::jsonb
        AND resource->'file' = '"LoincPartLink_Primary.csv"'::jsonb
        AND jsonb_typeof(resource->'sha256') = 'string'
        AND resource->>'sha256' ~ '^[0-9a-f]{64}$'
        AND jsonb_typeof(resource->'rows') = 'number'
        AND resource->>'rows' ~ '^[1-9][0-9]*$'
        AND resource->'rows' >= '1'::jsonb AND resource->'rows' <= '20000000'::jsonb
        AND jsonb_typeof(resource->'bytes') = 'number'
        AND resource->>'bytes' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'bytes' >= '0'::jsonb AND resource->'bytes' <= '4294967296'::jsonb
        AND resource->'version' = to_jsonb(expected_version)
        AND resource->'completeFile' = 'true'::jsonb,
        false
    )
$$;
ALTER TABLE loinc_releases
    ADD COLUMN primary_links_loaded boolean NOT NULL DEFAULT false,
    ADD COLUMN primary_link_source jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT loinc_primary_link_source_availability CHECK (COALESCE(
        jsonb_typeof(primary_link_source) = 'object'
        AND ((NOT primary_links_loaded AND primary_link_source = '{}'::jsonb)
            OR (primary_links_loaded AND loinc_primary_link_source_valid(primary_link_source, version))),
        false
    ));
CREATE TABLE loinc_primary_links (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    source_row bigint NOT NULL CHECK (source_row > 0),
    loinc_number text NOT NULL CHECK (octet_length(loinc_number) <= 255),
    long_common_name text NOT NULL,
    part_number text NOT NULL CHECK (octet_length(part_number) <= 255),
    part_name text NOT NULL,
    part_code_system text NOT NULL CHECK (octet_length(part_code_system) <= 255),
    part_type_name text NOT NULL CHECK (octet_length(part_type_name) <= 255),
    link_type_name text NOT NULL CHECK (octet_length(link_type_name) <= 255),
    property text NOT NULL CHECK (octet_length(property) <= 255),
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, source_row)
);
CREATE INDEX loinc_primary_link_term ON loinc_primary_links(code_system_id, loinc_number);
CREATE INDEX loinc_primary_link_axis ON loinc_primary_links(code_system_id, property, part_number, loinc_number)
    WHERE link_type_name = 'Primary' AND part_code_system = 'http://loinc.org';
CREATE TRIGGER loinc_primary_link_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON loinc_primary_links
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
