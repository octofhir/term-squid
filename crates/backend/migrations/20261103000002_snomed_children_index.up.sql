-- Descendant expansion traverses the same active inferred graph in the reverse direction.
CREATE INDEX snomed_inferred_children ON snomed_relationships(code_system_id, destination_code, source_code)
    WHERE active AND type_id = '116680003' AND characteristic_type_id = '900000000000011006';
