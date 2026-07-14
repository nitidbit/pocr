CREATE TABLE randomizations (
    id UUID PRIMARY KEY,
    study_id UUID NOT NULL REFERENCES studies(id) ON DELETE CASCADE,
    unit_hash TEXT NOT NULL,
    strata_key TEXT,
    arm JSONB NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_randomizations_study_unit ON randomizations(study_id, unit_hash);
CREATE INDEX idx_randomizations_study_id ON randomizations(study_id);
