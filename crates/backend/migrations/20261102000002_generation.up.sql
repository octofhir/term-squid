-- Transactional generation: no polling delay, sequence commit-order race or missed CLI write.
CREATE TABLE terminology_generation (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    generation bigint NOT NULL DEFAULT 1
);
INSERT INTO terminology_generation DEFAULT VALUES;
CREATE FUNCTION advance_terminology_generation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
    RETURN NULL;
END;
$$;
CREATE TRIGGER code_system_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON code_systems
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER value_set_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON value_sets
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
