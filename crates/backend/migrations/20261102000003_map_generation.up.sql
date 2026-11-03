-- A missing migration must fail closed rather than reuse maps without write tracking.
ALTER TABLE terminology_generation ADD COLUMN maps_tracked boolean NOT NULL DEFAULT true;
-- Prepared translation plans participate in the existing transactional generation protocol.
CREATE TRIGGER concept_map_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON concept_maps
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
