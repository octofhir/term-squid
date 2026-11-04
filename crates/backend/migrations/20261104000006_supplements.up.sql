-- Keep supplemental language lookup indexed without scanning native source archives.
ALTER TABLE concepts ADD COLUMN designations jsonb;
CREATE INDEX code_system_supplement_target ON code_systems
    ((split_part(content->>'supplements','|',1))) WHERE content_mode='supplement';
WITH RECURSIVE nodes(system_id,node,depth) AS (
    SELECT s.id,c,0 FROM code_systems s
    CROSS JOIN LATERAL jsonb_array_elements(COALESCE(s.content->'concept','[]')) c
    WHERE s.content_mode='supplement'
    UNION ALL
    SELECT n.system_id,c,n.depth+1 FROM nodes n
    CROSS JOIN LATERAL jsonb_array_elements(COALESCE(n.node->'concept','[]')) c
    WHERE n.depth<32
)
UPDATE concepts c SET designations=n.node->'designation' FROM nodes n
WHERE c.code_system_id=n.system_id AND c.code=n.node->>'code';
