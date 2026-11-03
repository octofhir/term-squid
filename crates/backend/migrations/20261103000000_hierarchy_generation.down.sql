DROP TRIGGER closure_generation ON closure_table;
DROP TRIGGER concept_projection_generation ON concepts;
ALTER TABLE terminology_generation DROP COLUMN hierarchies_tracked;
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
