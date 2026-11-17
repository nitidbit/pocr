import type {
	CategoricalField,
	ContinuousField,
	DatasetAttachment,
	DatasetField,
	DatasetList,
	DatasetReport,
	FieldBalance,
	PublishedReport,
	PublishedReportBase,
} from "#/api/datasets";

function categorical(
	name: string,
	labels: string[],
	counts: [(number | null)[], (number | null)[]],
	balance: FieldBalance = {},
	missing?: [number, number],
): CategoricalField {
	const categories = labels.map((label, index) => ({
		id: label,
		label,
		counts: [counts[0][index], counts[1][index]],
	}));
	return {
		...balance,
		analysis: "categorical",
		id: name,
		name,
		categories: missing
			? [...categories, { id: null, label: "Missing", counts: missing }]
			: categories,
	};
}

function continuous(
	name: string,
	stats: Pick<
		ContinuousField,
		"means" | "standardDeviations" | "denominators" | "decimals"
	> & { unit?: string; note?: string },
	balance: FieldBalance = {},
): DatasetField {
	return {
		...balance,
		analysis: "continuous",
		id: name,
		name,
		unit: stats.unit ?? null,
		note: stats.note ?? null,
		decimals: stats.decimals,
		means: stats.means,
		standardDeviations: stats.standardDeviations,
		denominators: stats.denominators,
	};
}

/** Synthetic cohort: every categorical field totals 120 per arm. */
export function syntheticReport(): DatasetReport {
	return {
		title: "Encounter Outcomes",
		description:
			"Admission characteristics, discharge details, and post-discharge outcomes by study arm.",
		smd: false,
		arms: [
			{ id: "intervention", name: "Enhanced Coordination", count: 120 },
			{ id: "control", name: "Usual Care", count: 120 },
		],
		fields: [
			continuous("LOS", {
				means: [4.8, 5.2],
				standardDeviations: [2.1, 2.4],
				denominators: [120, 120],
				unit: "days",
				decimals: 1,
			}),
			categorical(
				"Disposition",
				["Home Health Services", "Home or Self Care"],
				[
					[42, 78],
					[50, 70],
				],
			),
			categorical(
				"Discharge Day",
				[
					"Monday",
					"Tuesday",
					"Wednesday",
					"Thursday",
					"Friday",
					"Saturday",
					"Sunday",
				],
				[
					[18, 20, 21, 19, 22, 12, 8],
					[17, 19, 20, 21, 23, 11, 9],
				],
			),
			categorical(
				"Discharge Before Noon",
				["Yes", "No"],
				[
					[54, 66],
					[36, 84],
				],
			),
			categorical(
				"Admission Type",
				["Urgent", "Emergency", "Routine/Elective"],
				[
					[24, 72, 24],
					[26, 70, 24],
				],
			),
			continuous("DRG Weight", {
				decimals: 2,
				means: [1.42, 1.46],
				standardDeviations: [0.62, 0.68],
				denominators: [120, 120],
			}),
			categorical(
				"Readmission",
				["Yes", "No"],
				[
					[12, 108],
					[18, 102],
				],
			),
			continuous("Days to Readmission", {
				means: [14.2, 11.8],
				standardDeviations: [7.1, 6.4],
				denominators: [12, 18],
				unit: "days",
				decimals: 1,
				note: "Readmitted encounters only",
			}),
			categorical(
				"Return to ED",
				["Yes", "No"],
				[
					[18, 102],
					[24, 96],
				],
			),
		],
	};
}

/** Demographics with SMD; Race carries a suppressed category and an imbalance. */
export function demographicsReport(): DatasetReport {
	const { arms } = syntheticReport();
	return {
		title: "Demographics",
		description: "Patient characteristics at randomization by study arm.",
		smd: true,
		arms,
		fields: [
			continuous(
				"Age",
				{
					means: [61.4, 60.9],
					standardDeviations: [14.2, 15],
					denominators: [120, 119],
					unit: "years",
					decimals: 1,
				},
				{ smd: 0.03, imbalanced: false },
			),
			categorical(
				"Sex",
				["Female", "Male"],
				[
					[64, 56],
					[58, 62],
				],
				{ smd: 0.05, imbalanced: false },
			),
			categorical(
				"Race",
				[
					"White",
					"Black or African American",
					"Asian",
					"American Indian or Alaska Native",
				],
				[
					[58, 24, 26, null],
					[61, 22, 28, null],
				],
				{ smd: 0.18, imbalanced: true },
				[9, 7],
			),
			categorical(
				"Ethnicity",
				["Hispanic or Latino", "Not Hispanic or Latino"],
				[
					[30, 84],
					[27, 88],
				],
				{ smd: 0.04, imbalanced: false },
				[6, 5],
			),
			categorical(
				"Insurance",
				["Medicare", "Medicaid", "Commercial", "Self-pay"],
				[
					[58, 31, 26, 5],
					[61, 29, 24, 6],
				],
				{ smd: 0.06, imbalanced: false },
			),
			continuous(
				"BMI",
				{
					means: [28.4, 29.1],
					standardDeviations: [5.6, 6],
					denominators: [112, 108],
					unit: "kg/m²",
					decimals: 1,
				},
				{ smd: null, imbalanced: null },
			),
		],
	};
}

export function datasetAttachment(
	overrides: Partial<DatasetAttachment> = {},
): DatasetAttachment {
	return {
		id: "attachment-1",
		name: "encounter-outcomes",
		title: "Encounter Outcomes",
		description: "Discharge outcomes",
		status: "succeeded",
		completedAt: "2026-11-10T16:00:00Z",
		rowCount: 240,
		hasSnapshot: true,
		definitionAvailable: true,
		definitionChanged: false,
		canDownload: true,
		...overrides,
	};
}

export function datasetList(overrides: Partial<DatasetList> = {}): DatasetList {
	return { catalog: [], attachments: [], canManage: false, ...overrides };
}

const publishedBase: PublishedReportBase = {
	name: "encounter-outcomes",
	dataset: "encounter-outcomes",
	snapshotId: "snapshot-1",
	status: "succeeded",
	completedAt: "2026-11-10T16:00:00Z",
	definitionChanged: false,
};

export function publishedReport(
	overrides: Partial<Extract<PublishedReport, { state: "ready" }>> = {},
): PublishedReport {
	return {
		...publishedBase,
		state: "ready",
		report: syntheticReport(),
		...overrides,
	};
}

export function incompatibleReport(
	overrides: Partial<Extract<PublishedReport, { state: "incompatible" }>> = {},
): PublishedReport {
	return {
		...publishedBase,
		state: "incompatible",
		title: "Readmissions",
		incompatibility: { field: "los", reason: "unknownField" },
		...overrides,
	};
}
