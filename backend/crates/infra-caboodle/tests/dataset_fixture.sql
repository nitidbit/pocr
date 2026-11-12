-- Install only in a disposable SQL Server test database, never in Caboodle.
CREATE OR ALTER PROCEDURE dbo.PocrDatasetContractTest @context_json nvarchar(max)
AS
BEGIN
    SET NOCOUNT ON;
    SELECT CONVERT(uniqueidentifier, JSON_VALUE(j.value, '$.randomization_id')) AS randomization_id,
           c.note AS note,
           CAST(1.25 AS decimal(10,2)) AS amount,
           CAST(1 AS bit) AS flag
    FROM OPENJSON(@context_json) j
    CROSS APPLY OPENJSON(j.value, '$.context') WITH (note nvarchar(max) '$.note') c
    WHERE COALESCE(JSON_VALUE(j.value, '$.context.omit'), 'false') <> 'true';
    IF JSON_VALUE(@context_json, '$[0].context.extra_result') = 'true'
        SELECT 1 AS unexpected;
END;
