import type { BalanceResponse, StudyResponse } from "#/api/client";
import { syntheticReport } from "./fixtures";

export const DEMO_STUDY_ID = "dataset-demo";

const { arms } = syntheticReport();

export const demoStudy: StudyResponse = {
	id: DEMO_STUDY_ID,
	name: "Discharge Care Coordination",
	description:
		"Demo study comparing enhanced discharge coordination with usual care.",
	status: "active",
	randomizationConfig: {
		strategy: "block",
		arms: arms.map(({ id, name }) => ({ id, name, ratio: 1 })),
		keys: [{ EncounterId: null }],
		strata: [],
		blockSize: 4,
	},
	createdAt: "2026-10-01T12:00:00Z",
	updatedAt: "2026-11-03T12:00:00Z",
};

const total = arms.reduce((sum, arm) => sum + arm.count, 0);

export const demoCounts = {
	total,
	arms: arms.map(({ id, count }) => ({ armId: id, count })),
};

export const demoBalance: BalanceResponse = {
	study_id: DEMO_STUDY_ID,
	total_patients: total,
	arms: arms.map(({ id, count }) => ({ id, count })),
	variables: [],
};
