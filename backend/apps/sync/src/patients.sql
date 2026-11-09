SELECT DISTINCT ON (c.context_value) c.context_value #>> '{}' AS epic_patient_id, r.issued_at
FROM randomizations r
JOIN randomization_context c ON c.randomization_id = r.id
WHERE c.context_key = 'epic_patient_id'
ORDER BY c.context_value, r.issued_at ASC
