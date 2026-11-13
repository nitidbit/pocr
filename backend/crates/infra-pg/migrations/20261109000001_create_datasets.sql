CREATE TABLE study_datasets (
    id uuid PRIMARY KEY,
    study_id uuid NOT NULL REFERENCES studies(id) ON DELETE CASCADE,
    name text NOT NULL,
    UNIQUE (study_id, name)
);
CREATE TABLE dataset_runs (
    id uuid PRIMARY KEY,
    attachment_id uuid NOT NULL REFERENCES study_datasets(id) ON DELETE CASCADE,
    environment text NOT NULL CHECK (environment IN ('production', 'nonproduction')),
    definition jsonb NOT NULL,
    fingerprint text NOT NULL,
    status text NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    started_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    row_count bigint NOT NULL DEFAULT 0
);
CREATE INDEX dataset_runs_attachment ON dataset_runs(attachment_id, environment, started_at DESC);
CREATE TABLE dataset_rows (
    run_id uuid NOT NULL REFERENCES dataset_runs(id) ON DELETE CASCADE,
    randomization_id uuid NOT NULL REFERENCES randomizations(id) ON DELETE CASCADE,
    arm_id uuid NOT NULL,
    arm_name text NOT NULL,
    values jsonb,
    PRIMARY KEY (run_id, randomization_id)
);
CREATE INDEX dataset_rows_randomization ON dataset_rows(randomization_id);
-- Purging assignments invalidates complete snapshots, including in-flight cohorts.
CREATE FUNCTION invalidate_dataset_snapshots() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM dataset_runs dr
    USING study_datasets sd, (SELECT DISTINCT study_id, environment FROM purged) scope
    WHERE dr.attachment_id = sd.id AND sd.study_id = scope.study_id
      AND dr.environment = scope.environment;
    RETURN NULL;
END;
$$;
CREATE TRIGGER randomization_dataset_purge AFTER DELETE ON randomizations
REFERENCING OLD TABLE AS purged
FOR EACH STATEMENT EXECUTE FUNCTION invalidate_dataset_snapshots();
