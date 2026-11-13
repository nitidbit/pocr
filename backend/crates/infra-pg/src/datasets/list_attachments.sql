SELECT sd.id, sd.study_id, sd.name,
       COALESCE(ok.definition->>'title', sd.name) AS title,
       COALESCE(ok.definition->>'description', '') AS description,
       COALESCE(latest.status, 'pending') AS status, ok.completed_at,
       COALESCE(ok.row_count, 0) AS row_count, ok.fingerprint,
       ok.id IS NOT NULL AS has_snapshot
FROM study_datasets sd
LEFT JOIN LATERAL (
    SELECT status FROM dataset_runs
    WHERE attachment_id = sd.id AND environment = $2
    ORDER BY started_at DESC
    LIMIT 1
) latest ON true
LEFT JOIN LATERAL (
    SELECT * FROM dataset_runs
    WHERE attachment_id = sd.id AND environment = $2 AND status = 'succeeded'
    ORDER BY started_at DESC
    LIMIT 1
) ok ON true
WHERE sd.study_id = $1
ORDER BY title, sd.name
