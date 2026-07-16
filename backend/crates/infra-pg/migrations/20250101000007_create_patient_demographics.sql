CREATE TABLE patient_demographics (
    patient_id TEXT PRIMARY KEY,
    birth_date DATE,
    sex TEXT,
    races TEXT[],
    ethnicity TEXT,
    primary_language TEXT,
    smoking_status TEXT,
    insurance_type TEXT,
    bmi NUMERIC(5,2),
    synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
