import type { StudyResponse } from "#/api/client";

export const MOCK_STUDIES: StudyResponse[] = [
	{
		id: "study-001",
		name: "Perioperative Fluid Optimization in Cardiac Surgery",
		description:
			"A randomized controlled trial comparing goal-directed fluid therapy versus standard care in patients undergoing elective cardiac surgery.",
		status: "active",
		randomizationConfig: {
			strategy: "block",
			arms: [
				{ id: "arm-001", name: "Goal-Directed Therapy", ratio: 1 },
				{ id: "arm-002", name: "Standard Care", ratio: 1 },
			],
			keys: [{ PatientId: null }],
			strata: [
				{ id: "stratum-001", name: "Surgery Type", context: "Patient", path: "/surgeryType" },
			],
			blockSize: 4,
		},
		createdAt: "2024-01-10T08:00:00Z",
		updatedAt: "2025-05-01T14:30:00Z",
	},
	{
		id: "study-002",
		name: "ED Triage Acuity Prediction Algorithm Validation",
		description:
			"Cluster-randomized trial evaluating an ML-assisted triage system versus standard ESI triage in emergency department settings.",
		status: "active",
		randomizationConfig: {
			strategy: "block",
			arms: [
				{ id: "arm-003", name: "ML-Assisted Triage", ratio: 1 },
				{ id: "arm-004", name: "Standard ESI Triage", ratio: 1 },
			],
			keys: [{ EncounterId: null }],
			strata: [],
			blockSize: 6,
		},
		createdAt: "2024-04-22T10:15:00Z",
		updatedAt: "2025-04-18T09:00:00Z",
	},
	{
		id: "study-003",
		name: "Neonatal Sepsis Early Detection Biomarkers",
		description:
			"Planned trial comparing point-of-care biomarker panel versus standard blood culture for early neonatal sepsis detection.",
		status: "draft",
		randomizationConfig: {
			strategy: "simple",
			arms: [
				{ id: "arm-005", name: "Biomarker Panel", ratio: 2 },
				{ id: "arm-006", name: "Standard Blood Culture", ratio: 1 },
			],
			keys: [{ PatientId: null }],
			strata: [],
		},
		createdAt: "2025-03-01T11:00:00Z",
		updatedAt: "2025-05-10T16:45:00Z",
	},
	{
		id: "study-004",
		name: "Post-Stroke Rehabilitation Timing Optimization",
		description:
			"Completed trial evaluating early versus delayed intensive rehabilitation initiation in acute ischemic stroke patients.",
		status: "inactive",
		randomizationConfig: {
			strategy: "block",
			arms: [
				{ id: "arm-007", name: "Early Rehab", ratio: 1 },
				{ id: "arm-008", name: "Delayed Rehab", ratio: 1 },
			],
			keys: [{ PatientId: null }],
			strata: [
				{ id: "stratum-003", name: "Stroke Severity", context: "Patient", path: "/nihss" },
			],
			blockSize: 4,
		},
		createdAt: "2022-06-15T09:30:00Z",
		updatedAt: "2025-01-20T12:00:00Z",
	},
];
