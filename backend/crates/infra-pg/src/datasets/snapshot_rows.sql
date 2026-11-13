SELECT randomization_id, arm_id, arm_name, values
FROM dataset_rows
WHERE run_id = $1
ORDER BY randomization_id
