UPDATE dataset_runs
SET status = 'succeeded',
    completed_at = now(),
    row_count = (SELECT count(*) FROM dataset_rows WHERE run_id = $1)
WHERE id = $1 AND status = 'running'
  AND NOT EXISTS (
      SELECT 1 FROM dataset_rows WHERE run_id = $1 AND values IS NULL
  )
RETURNING row_count
