CREATE TABLE user_studies (
    user_id TEXT NOT NULL,
    study_id UUID NOT NULL REFERENCES studies(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, study_id)
);
