SELECT dr.id, dr.definition, dr.completed_at,
       (
           SELECT status FROM dataset_runs
           WHERE attachment_id = sd.id AND environment = $3
           ORDER BY started_at DESC
           LIMIT 1
       ) AS status
FROM dataset_runs dr
JOIN study_datasets sd ON sd.id = dr.attachment_id
WHERE sd.study_id = $1 AND sd.name = $2
  AND dr.environment = $3 AND dr.status = 'succeeded'
ORDER BY dr.started_at DESC
LIMIT 1
