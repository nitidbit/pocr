DELETE FROM dataset_runs
WHERE attachment_id = $1 AND environment = $2 AND id <> $3
  AND id IS DISTINCT FROM (
      SELECT id FROM dataset_runs
      WHERE attachment_id = $1 AND environment = $2 AND status = 'succeeded'
      ORDER BY started_at DESC
      LIMIT 1
  )
