-- Hierarchy snapshots read stored concepts/closure, including projection-only writes.
ALTER TABLE terminology_generation ADD COLUMN hierarchies_tracked boolean NOT NULL DEFAULT true;
CREATE TRIGGER concept_projection_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON concepts
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
CREATE TRIGGER closure_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON closure_table
    FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation();
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
