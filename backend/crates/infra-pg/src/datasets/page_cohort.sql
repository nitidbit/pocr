SELECT r.id, r.issued_at,
       COALESCE((
           SELECT jsonb_object_agg(context_key, context_value)
           FROM randomization_context
           WHERE randomization_id = r.id
       ), '{}'::jsonb) AS context
FROM dataset_rows d
JOIN randomizations r ON r.id = d.randomization_id
WHERE d.run_id = $1 AND ($2::uuid IS NULL OR r.id > $2)
ORDER BY r.id
LIMIT $3
