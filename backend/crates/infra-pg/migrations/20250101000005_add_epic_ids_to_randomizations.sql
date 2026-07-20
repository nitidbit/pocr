ALTER TABLE randomizations
    ADD COLUMN epic_patient_id TEXT,
    ADD COLUMN epic_csn TEXT,
    ADD COLUMN epic_user_id TEXT;

CREATE INDEX idx_randomizations_epic_patient_id ON randomizations(epic_patient_id);
