-- Database deployment example only. Never executed or uploaded by POCR.
-- The result columns must match dataset.yaml in this directory: randomization_id
-- plus each declared column, with no other identifier columns.
-- Grant the sync account EXECUTE on this procedure.
-- Verify source mappings before deployment. Requires an existing pocr schema.
-- DB OWNER TO CONFIRM: FlowsheetValueFact.TakenInstant must use the same time
-- basis as randomization_time (UTC). If TakenInstant is local time, convert it
-- (for example AT TIME ZONE) in the two flowsheet comparisons below.
CREATE OR ALTER PROCEDURE [pocr].[EncounterBalance]
    @context_json nvarchar(max)
AS
BEGIN
    SET NOCOUNT ON;

    IF @context_json IS NULL OR ISJSON(@context_json) <> 1
        THROW 50001, 'Expected a JSON batch.', 1;

    -- ISJSON alone also accepts objects; the common envelope must be an array.
    IF LEFT(LTRIM(REPLACE(REPLACE(REPLACE(@context_json,
        CHAR(9), ' '), CHAR(10), ' '), CHAR(13), ' ')), 1) <> '['
        THROW 50002, 'Expected a JSON array.', 1;

    WITH inputs AS (
        SELECT randomization_id, encounter_id, randomization_time
        FROM OPENJSON(@context_json) WITH (
            randomization_id uniqueidentifier '$.randomization_id',
            encounter_id nvarchar(100) '$.context.epic_csn',
            randomization_time datetime2 '$.randomization_time'
        )
    )
    SELECT input.randomization_id,
           CASE
               WHEN pd.BirthDate <= CAST(input.randomization_time AS date) THEN
                   DATEDIFF(year, pd.BirthDate, input.randomization_time)
                       - CASE
                             WHEN DATEADD(year, DATEDIFF(year, pd.BirthDate, input.randomization_time), pd.BirthDate)
                                 > CAST(input.randomization_time AS date) THEN 1
                             ELSE 0 END
               END                      AS age,
           pd.Sex                       AS sex,
           races.value                  AS race,
           pd.Ethnicity                 AS ethnicity,
           pd.PreferredLanguage         AS primary_language,
           coverage.PayorFinancialClass AS insurance_type,
           pd.SmokingStatus             AS smoking_status,
           CASE
               WHEN weight.value > 0 AND height.value > 0 THEN
                   (weight.value / 35.274) / NULLIF(POWER(height.value * 0.0254, 2), 0)
               END                      AS bmi
    FROM inputs AS input
             LEFT JOIN EncounterFact AS encounter
                       ON encounter.EncounterEpicCsn = input.encounter_id
             LEFT JOIN PatientDim AS pd ON pd.PatientKey = encounter.PatientKey
             LEFT JOIN CoverageDim AS coverage ON coverage.CoverageKey = encounter.CoverageKey
             OUTER APPLY (SELECT TOP (1) TRY_CONVERT(float, fvf.Value) AS value
                          FROM FlowsheetValueFact AS fvf
                                   INNER JOIN FlowsheetRowDim AS row_dim
                                              ON row_dim.FlowsheetRowKey = fvf.FlowsheetRowKey
                          WHERE fvf.EncounterKey = encounter.EncounterKey
                            AND row_dim.FlowsheetRowEpicId = '14'
                            AND fvf.TakenInstant <= input.randomization_time
                          ORDER BY fvf.TakenInstant DESC) AS weight
             OUTER APPLY (SELECT TOP (1) TRY_CONVERT(float, fvf.Value) AS value
                          FROM FlowsheetValueFact AS fvf
                                   INNER JOIN FlowsheetRowDim AS row_dim
                                              ON row_dim.FlowsheetRowKey = fvf.FlowsheetRowKey
                          WHERE fvf.EncounterKey = encounter.EncounterKey
                            AND row_dim.FlowsheetRowEpicId = '11'
                            AND fvf.TakenInstant <= input.randomization_time
                          ORDER BY fvf.TakenInstant DESC) AS height
             OUTER APPLY (
        -- Several distinct races collapse to 'Multiracial'.
        -- DB OWNER TO DECIDE: whether to exclude 'Unknown'/'Declined' names
        -- (add a predicate on bridge.Name below) so they do not count as a race.
        SELECT CASE WHEN COUNT(*) > 1 THEN N'Multiracial' ELSE MAX(race.Name) END AS value
        FROM (SELECT DISTINCT bridge.Name
              FROM PatientRaceBridge AS bridge
              WHERE bridge.PatientRaceComboKey = pd.PatientRaceComboKey
                AND bridge.Name IS NOT NULL) AS race
    ) AS races;
END;
