DROP TRIGGER concept_map_generation ON concept_maps;
ALTER TABLE terminology_generation DROP COLUMN maps_tracked;
UPDATE terminology_generation SET generation = generation + 1 WHERE singleton;
