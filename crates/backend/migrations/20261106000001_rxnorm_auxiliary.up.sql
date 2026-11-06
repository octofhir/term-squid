-- Source availability is declared by a verified monthly cohort, never inferred from rows.
CREATE FUNCTION rxnorm_auxiliary_source_valid(
    resource jsonb, expected_file text, expected_version text, minimum_rows bigint
) RETURNS boolean LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT COALESCE(
        jsonb_typeof(resource) = 'object'
        AND octet_length(resource::text) <= 4096
        AND resource ?& ARRAY['file','sha256','rows','bytes','version','fullMonthly']
        AND resource - 'file' - 'sha256' - 'rows' - 'bytes' - 'version' - 'fullMonthly' = '{}'::jsonb
        AND resource->'file' = to_jsonb(expected_file)
        AND jsonb_typeof(resource->'sha256') = 'string'
        AND resource->>'sha256' ~ '^[0-9a-f]{64}$'
        AND jsonb_typeof(resource->'rows') = 'number'
        AND resource->>'rows' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'rows' >= to_jsonb(minimum_rows)
        AND resource->'rows' <= '20000000'::jsonb
        AND jsonb_typeof(resource->'bytes') = 'number'
        AND resource->>'bytes' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'bytes' >= '0'::jsonb
        AND resource->'bytes' <= '4294967296'::jsonb
        AND resource->'version' = to_jsonb(expected_version)
        AND resource->'fullMonthly' = 'true'::jsonb,
        false
    )
$$;

ALTER TABLE rxnorm_releases
    ADD COLUMN semantic_types_loaded boolean NOT NULL DEFAULT false,
    ADD COLUMN attributes_loaded boolean NOT NULL DEFAULT false,
    ADD COLUMN auxiliary_sources jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT rxnorm_auxiliary_source_availability CHECK (COALESCE(
        jsonb_typeof(auxiliary_sources) = 'object'
        AND octet_length(auxiliary_sources::text) <= 8192
        AND auxiliary_sources - 'RXNSTY.RRF' - 'RXNSAT.RRF' = '{}'::jsonb
        AND semantic_types_loaded = (auxiliary_sources ? 'RXNSTY.RRF')
        AND attributes_loaded = (auxiliary_sources ? 'RXNSAT.RRF')
        AND (NOT semantic_types_loaded OR rxnorm_auxiliary_source_valid(
            auxiliary_sources->'RXNSTY.RRF', 'RXNSTY.RRF', version, 1
        ))
        AND (NOT attributes_loaded OR rxnorm_auxiliary_source_valid(
            auxiliary_sources->'RXNSAT.RRF', 'RXNSAT.RRF', version, 0
        )), false
    ));

-- Source ordinals preserve the original cohort without indexing unbounded raw values.
CREATE TABLE rxnorm_semantic_types (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    source_row bigint NOT NULL CHECK (source_row > 0),
    rxcui text NOT NULL CHECK (octet_length(rxcui) <= 255),
    tui text NOT NULL CHECK (octet_length(tui) <= 255),
    stn text NOT NULL CHECK (octet_length(stn) <= 255),
    sty text NOT NULL,
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, source_row)
);
CREATE INDEX rxnorm_semantic_type_concept ON rxnorm_semantic_types(code_system_id, rxcui);
CREATE INDEX rxnorm_semantic_type_tui ON rxnorm_semantic_types(code_system_id, tui, rxcui);
CREATE INDEX rxnorm_semantic_type_tree ON rxnorm_semantic_types(code_system_id, stn, rxcui);

CREATE TABLE rxnorm_attributes (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    source_row bigint NOT NULL CHECK (source_row > 0),
    rxcui text NOT NULL CHECK (octet_length(rxcui) <= 255),
    rxaui text NOT NULL,
    stype text NOT NULL,
    code text NOT NULL,
    atui text NOT NULL,
    atn text NOT NULL CHECK (octet_length(atn) <= 255),
    sab text NOT NULL CHECK (octet_length(sab) <= 255),
    atv text NOT NULL,
    suppress text NOT NULL CHECK (suppress IN ('N', 'O', 'Y')),
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, source_row)
);
CREATE INDEX rxnorm_attribute_concept ON rxnorm_attributes(code_system_id, rxcui);
CREATE INDEX rxnorm_attribute_kind ON rxnorm_attributes(code_system_id, rxcui, atn, sab);

CREATE TRIGGER rxnorm_semantic_type_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_semantic_types
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER rxnorm_attribute_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_attributes
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
