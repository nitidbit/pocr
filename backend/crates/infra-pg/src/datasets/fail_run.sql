UPDATE dataset_runs
SET status = 'failed',
    completed_at = now()
WHERE id = $1 AND status = 'running'
RETURNING id
