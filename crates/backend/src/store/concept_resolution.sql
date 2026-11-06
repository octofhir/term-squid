SELECT s.url, s.native_provider, s.name, s.version, CASE WHEN s.native_provider='rxnorm-rrf' THEN 'not-present' WHEN s.native_provider IS NOT NULL THEN 'complete' ELSE s.content_mode END AS content_mode, c.code, c.display, c.definition, c.properties
            FROM (SELECT id, url, name, version, content_mode, case_sensitive, native_provider FROM code_systems
                  WHERE ($4::uuid IS NOT NULL AND id = $4) OR ($4::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2))
                  ORDER BY updated_at DESC, id LIMIT 1) s
            LEFT JOIN LATERAL (
                SELECT code, display, definition, properties FROM concepts
                WHERE code_system_id = s.id AND
                    ((s.case_sensitive AND code = $3) OR
                     (NOT s.case_sensitive AND lower(code) = lower($3)))
                LIMIT 1
            ) c ON true
