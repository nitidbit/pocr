INSERT INTO dataset_rows (run_id, randomization_id, arm_id, arm_name)
SELECT $1, r.id, (r.arm->>'id')::uuid, r.arm->>'name'
FROM randomizations r
JOIN study_datasets sd ON sd.study_id = r.study_id
WHERE sd.id = $2 AND r.environment = $3
