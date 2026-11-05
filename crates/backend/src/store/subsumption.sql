WITH RECURSIVE selected AS MATERIALIZED (
    SELECT id, url, version, hierarchy_meaning, case_sensitive, native_provider, content_mode FROM code_systems
    WHERE ($5::uuid IS NOT NULL AND id = $5) OR
          ($5::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2))
    ORDER BY updated_at DESC, id LIMIT 1
), a AS MATERIALIZED (
    SELECT c.code FROM concepts c JOIN selected s ON c.code_system_id = s.id
    WHERE s.case_sensitive AND c.code = $3
    UNION ALL
    SELECT c.code FROM concepts c JOIN selected s ON c.code_system_id = s.id
    WHERE NOT s.case_sensitive AND lower(c.code) = lower($3)
    LIMIT 1
), b AS MATERIALIZED (
    SELECT c.code FROM concepts c JOIN selected s ON c.code_system_id = s.id
    WHERE s.case_sensitive AND c.code = $4
    UNION ALL
    SELECT c.code FROM concepts c JOIN selected s ON c.code_system_id = s.id
    WHERE NOT s.case_sensitive AND lower(c.code) = lower($4)
    LIMIT 1
), walk(side, code) AS (
    SELECT seeds.side, seeds.code::text FROM (
        SELECT 0 AS side, code FROM a UNION ALL SELECT 1, code FROM b
    ) seeds
    WHERE EXISTS (SELECT 1 FROM selected WHERE native_provider = 'snomed-rf2' AND hierarchy_meaning = 'is-a')
      AND EXISTS (SELECT 1 FROM a, b WHERE a.code <> b.code)
    UNION
    SELECT w.side, r.destination_code::text FROM walk w
    JOIN selected s ON s.native_provider = 'snomed-rf2'
    JOIN snomed_relationships r ON r.code_system_id = s.id AND r.source_code = w.code
    WHERE r.active AND r.type_id = '116680003' AND r.characteristic_type_id = '900000000000011006'
), bounded AS MATERIALIZED (
    -- Recursive UNION deduplicates cycles; only this bounded consumer reads it.
    SELECT side, code FROM walk LIMIT 8193
)
SELECT s.url, s.version, s.native_provider, (COALESCE(s.content_mode = 'complete', false) OR s.native_provider IS NOT NULL) AS complete, s.hierarchy_meaning, a.code AS code_a, b.code AS code_b,
       CASE WHEN s.hierarchy_meaning = 'is-a' AND a.code <> b.code THEN
           CASE WHEN s.native_provider = 'snomed-rf2' THEN
               CASE WHEN EXISTS (SELECT 1 FROM bounded WHERE side = 1 AND code = a.code) THEN true
                    WHEN EXISTS (SELECT 1 FROM bounded WHERE side = 0 AND code = b.code) THEN false END
           ELSE
               CASE WHEN EXISTS (SELECT 1 FROM closure_table WHERE code_system_id = s.id AND ancestor_code = a.code AND descendant_code = b.code) THEN true
                    WHEN EXISTS (SELECT 1 FROM closure_table WHERE code_system_id = s.id AND ancestor_code = b.code AND descendant_code = a.code) THEN false END
           END
       END AS relationship,
       CASE WHEN s.native_provider = 'snomed-rf2' THEN (SELECT count(*) > 8192 FROM bounded)
            ELSE false END AS native_limited
FROM selected s LEFT JOIN a ON true LEFT JOIN b ON true
