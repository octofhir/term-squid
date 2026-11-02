-- Backfill nested concepts and transitive is-a closure for existing CLI imports.
-- Statement-local recursion detects cycles by path and caps depth at 32.
WITH RECURSIVE nodes AS (
    SELECT id AS cs, c AS concept FROM code_systems,
        LATERAL jsonb_array_elements(COALESCE(content->'concept', '[]'::jsonb)) c
    UNION ALL
    SELECT cs, child FROM nodes,
        LATERAL jsonb_array_elements(COALESCE(concept->'concept', '[]'::jsonb)) child
)
INSERT INTO concepts (code_system_id, code, display, definition, properties)
SELECT cs, concept->>'code', concept->>'display', concept->>'definition', concept->'property'
FROM nodes WHERE concept->>'code' IS NOT NULL
ON CONFLICT (code_system_id, code) DO NOTHING;

WITH RECURSIVE nodes AS (
    SELECT id AS cs, c AS concept, NULL::text AS parent FROM code_systems,
        LATERAL jsonb_array_elements(COALESCE(content->'concept', '[]'::jsonb)) c
    UNION ALL
    SELECT cs, child, concept->>'code' FROM nodes,
        LATERAL jsonb_array_elements(COALESCE(concept->'concept', '[]'::jsonb)) child
), edges AS (
    SELECT cs, parent AS ancestor, concept->>'code' AS descendant FROM nodes WHERE parent IS NOT NULL
    UNION
    SELECT cs, p->>'valueCode', concept->>'code' FROM nodes,
        LATERAL jsonb_array_elements(COALESCE(concept->'property', '[]'::jsonb)) p
        WHERE p->>'code' = 'parent' AND p->>'valueCode' IS NOT NULL
    UNION
    SELECT cs, concept->>'code', p->>'valueCode' FROM nodes,
        LATERAL jsonb_array_elements(COALESCE(concept->'property', '[]'::jsonb)) p
        WHERE p->>'code' = 'child' AND p->>'valueCode' IS NOT NULL
), paths AS (
    SELECT cs, ancestor, descendant, 1 AS depth, ARRAY[ancestor, descendant] AS visited FROM edges
    UNION ALL
    SELECT p.cs, p.ancestor, e.descendant, p.depth + 1, p.visited || e.descendant
    FROM paths p JOIN edges e ON e.cs = p.cs AND e.ancestor = p.descendant
    WHERE p.depth < 32 AND NOT e.descendant = ANY(p.visited)
)
INSERT INTO closure_table (code_system_id, ancestor_code, descendant_code, depth)
SELECT cs, ancestor, descendant, MIN(depth) FROM paths
WHERE ancestor <> descendant GROUP BY cs, ancestor, descendant
ON CONFLICT (code_system_id, ancestor_code, descendant_code) DO UPDATE SET depth = EXCLUDED.depth;
