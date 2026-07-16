-- Partition randomization records by deployment environment so that
-- non-production (test/training) assignments never mix with production
-- trial data. Existing rows predate the split and are production.
ALTER TABLE randomizations
    ADD COLUMN environment TEXT NOT NULL DEFAULT 'production'
        CHECK (environment IN ('production', 'nonproduction'));

-- Uniqueness (idempotency) is now per environment: the same subject may be
-- randomized once in production and independently once in non-production.
DROP INDEX idx_randomizations_study_unit;
CREATE UNIQUE INDEX idx_randomizations_study_env_unit
    ON randomizations (study_id, environment, unit_hash);

-- Support environment-scoped listing and non-production purges.
CREATE INDEX idx_randomizations_study_env
    ON randomizations (study_id, environment);
