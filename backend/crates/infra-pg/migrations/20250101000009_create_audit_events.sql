-- Append-only audit log. study_id is intentionally NOT a foreign key: audit
-- rows must outlive study deletion so the "who deleted this study, and when"
-- record survives in a regulated clinical-trial context.
CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    study_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_events_study_created ON audit_events(study_id, created_at DESC);
