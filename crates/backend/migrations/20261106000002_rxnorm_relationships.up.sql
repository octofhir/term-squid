-- Relationship source context is independent of the earlier optional semantic/SAT cohort.
CREATE FUNCTION rxnorm_relationship_source_valid(resource jsonb, expected_version text)
RETURNS boolean LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT COALESCE(
        jsonb_typeof(resource) = 'object'
        AND octet_length(resource::text) <= 4096
        AND resource ?& ARRAY['file','sha256','rows','bytes','version','fullMonthly']
        AND resource - 'file' - 'sha256' - 'rows' - 'bytes' - 'version' - 'fullMonthly' = '{}'::jsonb
        AND resource->'file' = '"RXNREL.RRF"'::jsonb
        AND jsonb_typeof(resource->'sha256') = 'string'
        AND resource->>'sha256' ~ '^[0-9a-f]{64}$'
        AND jsonb_typeof(resource->'rows') = 'number'
        AND resource->>'rows' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'rows' >= '0'::jsonb AND resource->'rows' <= '20000000'::jsonb
        AND jsonb_typeof(resource->'bytes') = 'number'
        AND resource->>'bytes' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'bytes' >= '0'::jsonb AND resource->'bytes' <= '4294967296'::jsonb
        AND resource->'version' = to_jsonb(expected_version)
        AND resource->'fullMonthly' = 'true'::jsonb,
        false
    )
$$;

ALTER TABLE rxnorm_releases
    ADD COLUMN relationships_loaded boolean NOT NULL DEFAULT false,
    ADD COLUMN relationship_source jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT rxnorm_relationship_source_availability CHECK (COALESCE(
        jsonb_typeof(relationship_source) = 'object'
        AND ((NOT relationships_loaded AND relationship_source = '{}'::jsonb)
            OR (relationships_loaded AND rxnorm_relationship_source_valid(relationship_source, version))),
        false
    ));

-- Original source ordinals retain duplicate/inverse facts and opaque UMLS endpoints.
CREATE TABLE rxnorm_relationships (
    code_system_id uuid NOT NULL REFERENCES code_systems(id) ON DELETE CASCADE,
    source_row bigint NOT NULL CHECK (source_row > 0),
    rxcui1 text NOT NULL CHECK (octet_length(rxcui1) <= 255),
    rxaui1 text NOT NULL CHECK (octet_length(rxaui1) <= 255),
    stype1 text NOT NULL CHECK (octet_length(stype1) <= 255),
    rel text NOT NULL CHECK (octet_length(rel) <= 255),
    rxcui2 text NOT NULL CHECK (octet_length(rxcui2) <= 255),
    rxaui2 text NOT NULL CHECK (octet_length(rxaui2) <= 255),
    stype2 text NOT NULL CHECK (octet_length(stype2) <= 255),
    rela text NOT NULL CHECK (octet_length(rela) <= 255),
    rui text NOT NULL CHECK (octet_length(rui) <= 255),
    srui text NOT NULL CHECK (octet_length(srui) <= 255),
    sab text NOT NULL CHECK (octet_length(sab) <= 255),
    sl text NOT NULL CHECK (octet_length(sl) <= 255),
    rg text NOT NULL CHECK (octet_length(rg) <= 255),
    dir text NOT NULL CHECK (octet_length(dir) <= 255),
    suppress text NOT NULL CHECK (suppress IN ('', 'N', 'Y', 'E', 'O')),
    cvf text NOT NULL CHECK (octet_length(cvf) <= 255),
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, source_row)
);
CREATE INDEX rxnorm_relationship_cui_rel ON rxnorm_relationships(code_system_id, rxcui2, rel, rxcui1)
    WHERE suppress IN ('', 'N');
CREATE INDEX rxnorm_relationship_cui_rela ON rxnorm_relationships(code_system_id, rxcui2, rela, rxcui1)
    WHERE suppress IN ('', 'N');
CREATE INDEX rxnorm_relationship_aui_rel ON rxnorm_relationships(code_system_id, rxaui2, rel, rxaui1)
    WHERE suppress IN ('', 'N');
CREATE INDEX rxnorm_relationship_aui_rela ON rxnorm_relationships(code_system_id, rxaui2, rela, rxaui1)
    WHERE suppress IN ('', 'N');
CREATE TRIGGER rxnorm_relationship_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_relationships
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
