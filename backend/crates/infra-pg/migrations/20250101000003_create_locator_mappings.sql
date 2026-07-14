CREATE TABLE locator_mappings (
    locator BIGINT PRIMARY KEY,
    study_id UUID NOT NULL REFERENCES studies(id) ON DELETE CASCADE
);

CREATE INDEX idx_locator_mappings_study_id ON locator_mappings(study_id);
