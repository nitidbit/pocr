-- Database deployment example only. Never executed or uploaded by POCR.
-- The result columns must match dataset.yaml in this directory: randomization_id
-- plus each declared column, with no other identifier columns.
create procedure [pocr].[EncounterBalance]
    @context_json nvarchar(max)
as
begin
    set transaction isolation level snapshot;
    set nocount on;

    drop table if exists #context;
    with input as (
        select randomization_id, EncounterEpicCsn, RandomizationTime
        from openjson(@context_json)
                      with (
                          randomization_id uniqueidentifier '$.randomization_id',
                          EncounterEpicCsn numeric(18) '$.context.epic_csn',
                          RandomizationTime datetime2 '$.randomization_time'
                          )
    )
    select randomization_id, RandomizationTime, EncounterKey, PatientKey
    into #context
    from input
             left join CDW.FilteredAccess.EncounterFact ef on ef.EncounterEpicCsn = input.EncounterEpicCsn;
    create clustered index idx_encounter on #context(EncounterKey);
    create index idx_patient on #context(PatientKey);
    -- select * from #context;

    drop table if exists #race;
    with PatientRace as (
        select ctx.randomization_id, prb.Name as Race
        from #context ctx
                 join CDW.FilteredAccess.PatientDim pd on pd.PatientKey = ctx.PatientKey
                 join CDW.FilteredAccess.PatientRaceBridge prb on prb.PatientRaceComboKey = pd.PatientRaceComboKey
    )
    select randomization_id, iif(count(*) > 1, N'Multiracial', max(Race)) as Race
    into #race
    from PatientRace
    group by randomization_id;
    -- select * from #race;

    drop table if exists #measurements;
    with
        Flowsheets as (
            select
                ctx.randomization_id,
                fvf.FlowsheetRowEpicId,
                fvf.NumericValue,
                row_number() over (
                    partition by ctx.randomization_id, fvf.FlowsheetRowEpicId
                    order by fvf.Instant desc
                    ) as rn
            from #context ctx
                     join CDW.FilteredAccess.FlowsheetValueFact fvf on fvf.PatientKey = ctx.PatientKey
            where fvf.FlowsheetRowEpicId IN ('11', '14')
              and fvf.Instant < ctx.RandomizationTime
        ),
        Measurements AS (
            select
                randomization_id,
                MAX(case when FlowsheetRowEpicId = '11' then NumericValue end) as Height,
                MAX(case when FlowsheetRowEpicId = '14' then NumericValue end) as Weight
            from Flowsheets
            where rn = 1
            group by randomization_id
        )
    select
        randomization_id,
        Height,
        Weight
    into #measurements
    from measurements;
    -- select * from #measurements;

    select
        ctx.randomization_id,
        floor(datediff(day, pat.BirthDate, getdate()) / 365.25)          as Age,
        pat.Sex,
        race.Race,
        pat.Ethnicity,
        pat.PreferredLanguage,
        cvg.PayorFinancialClass,
        pat.SmokingStatus,
        (flo.Weight / 35.274) / nullif(power(flo.Height * 0.0254, 2), 0) as BMI
    from #context ctx
             left join CDW.FilteredAccess.EncounterFact enc on enc.EncounterKey = ctx.EncounterKey
             left join CDW.FilteredAccess.PatientDim pat on pat.PatientKey = ctx.PatientKey
             left join CDW.FilteredAccess.CoverageDim cvg on cvg.CoverageKey = enc.PrimaryCoverageKey
             left join #race race on race.randomization_id = ctx.randomization_id
             left join #measurements flo on flo.randomization_id = ctx.randomization_id;
end;
