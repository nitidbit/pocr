CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    study_id UUID NOT NULL REFERENCES studies(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_events_study_created ON audit_events(study_id, created_at DESC);
