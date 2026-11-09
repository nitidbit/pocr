-- Context is an assignment snapshot. The parent supplies study/environment scope.
CREATE TABLE randomization_context (
    randomization_id UUID NOT NULL REFERENCES randomizations(id) ON DELETE CASCADE,
    context_key TEXT NOT NULL CHECK (btrim(context_key) <> ''),
    context_value JSONB NOT NULL,
    PRIMARY KEY (randomization_id, context_key),
    CONSTRAINT context_identifiers_are_strings CHECK (
        context_key NOT IN ('epic_patient_id', 'epic_csn', 'epic_user_id',
                            'fhir_patient_id', 'fhir_encounter_id',
                            'fhir_user_reference', 'fhir_server')
        OR jsonb_typeof(context_value) = 'string'
    )
);
