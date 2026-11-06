-- Availability applies only to the explicitly attested concept-only Simple refsets.
CREATE FUNCTION snomed_simple_refset_source_valid(resource jsonb, expected_release text)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE AS $$
DECLARE
    item jsonb;
    previous_id text;
BEGIN
    IF NOT COALESCE(
        jsonb_typeof(resource) = 'object'
        AND octet_length(resource::text) <= 16384
        AND resource ?& ARRAY['file','sha256','rows','bytes','release','completeSnapshot','conceptOnly','refsetIds']
        AND resource - 'file' - 'sha256' - 'rows' - 'bytes' - 'release' - 'completeSnapshot' - 'conceptOnly' - 'refsetIds' = '{}'::jsonb
        AND jsonb_typeof(resource->'file') = 'string'
        AND resource->>'file' !~ '[/\\]'
        AND position('SimpleSnapshot' IN resource->>'file') > 0
        AND position('Full' IN resource->>'file') = 0
        AND position('Delta' IN resource->>'file') = 0
        AND right(resource->>'file', 13) = '_' || expected_release || '.txt'
        AND expected_release ~ '^[0-9]{8}$'
        AND resource->'release' = to_jsonb(expected_release)
        AND jsonb_typeof(resource->'sha256') = 'string'
        AND resource->>'sha256' ~ '^[0-9a-f]{64}$'
        AND jsonb_typeof(resource->'rows') = 'number'
        AND resource->>'rows' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'rows' >= '0'::jsonb AND resource->'rows' <= '20000000'::jsonb
        AND jsonb_typeof(resource->'bytes') = 'number'
        AND resource->>'bytes' ~ '^(0|[1-9][0-9]*)$'
        AND resource->'bytes' >= '0'::jsonb AND resource->'bytes' <= '2147483648'::jsonb
        AND resource->'completeSnapshot' = 'true'::jsonb
        AND resource->'conceptOnly' = 'true'::jsonb
        AND jsonb_typeof(resource->'refsetIds') = 'array', false
    ) THEN RETURN false; END IF;
    IF jsonb_array_length(resource->'refsetIds') NOT BETWEEN 1 AND 64 THEN
        RETURN false;
    END IF;
    FOR item IN SELECT value FROM jsonb_array_elements(resource->'refsetIds') LOOP
        IF jsonb_typeof(item) <> 'string' OR item #>> '{}' !~ '^[1-9][0-9]{5,17}$'
            OR item #>> '{}' = '446609009'
            OR (previous_id IS NOT NULL AND (item #>> '{}') COLLATE "C" <= previous_id COLLATE "C")
        THEN RETURN false; END IF;
        previous_id := item #>> '{}';
    END LOOP;
    RETURN true;
END $$;

ALTER TABLE snomed_releases
    ADD COLUMN simple_refsets_loaded boolean NOT NULL DEFAULT false,
    ADD COLUMN simple_refset_source jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT snomed_simple_refset_source_availability CHECK (COALESCE(
        (NOT simple_refsets_loaded AND simple_refset_source = '{}'::jsonb)
        OR (simple_refsets_loaded AND snomed_simple_refset_source_valid(
            simple_refset_source, to_char(release_date, 'YYYYMMDD')
        )), false
    ));

CREATE TABLE snomed_simple_refsets (
    code_system_id uuid NOT NULL REFERENCES snomed_releases(code_system_id) ON DELETE CASCADE,
    refset_id text NOT NULL CHECK (refset_id ~ '^[1-9][0-9]{5,17}$'),
    PRIMARY KEY (code_system_id, refset_id)
);
CREATE TABLE snomed_simple_members (
    code_system_id uuid NOT NULL,
    id uuid NOT NULL,
    effective_time date NOT NULL,
    active boolean NOT NULL,
    module_id text NOT NULL CHECK (module_id ~ '^[1-9][0-9]{0,17}$'),
    refset_id text NOT NULL,
    referenced_component_id text NOT NULL CHECK (referenced_component_id ~ '^[1-9][0-9]{0,17}$'),
    fields jsonb NOT NULL CHECK (jsonb_typeof(fields) = 'object'),
    PRIMARY KEY (code_system_id, id),
    FOREIGN KEY (code_system_id, refset_id) REFERENCES snomed_simple_refsets(code_system_id, refset_id) ON DELETE CASCADE
);
CREATE INDEX snomed_simple_membership ON snomed_simple_members(code_system_id, refset_id, referenced_component_id) WHERE active;
CREATE INDEX snomed_simple_member_concept ON snomed_simple_members(code_system_id, referenced_component_id, refset_id) WHERE active;
CREATE TRIGGER snomed_simple_refset_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_simple_refsets
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER snomed_simple_member_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON snomed_simple_members
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
