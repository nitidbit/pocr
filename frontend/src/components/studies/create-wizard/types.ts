import type { KeyType, RandomizationStrategy } from "#/lib/types/randomization";

/** Full form state for the multi-step study creation wizard. */
export interface CreateStudyFormData {
	// Step 1: Study Information
	title: string;
	shortName: string;
	irbNumber: string;
	irbException: "none" | "waiver" | "exempt";
	protocolId: string;
	protocolLink: string;
	description: string;

	// Step 2: Team & Timeline
	piName: string;
	piEmail: string;
	coordinatorName: string;
	coordinatorEmail: string;
	statisticianName: string;
	statisticianEmail: string;
	sites: Array<{ id: string; name: string; enrollmentCap: string }>;
	startDate: string;
	endDate: string;
	targetEnrollment: string;

	// Step 3: Randomization Design
	strategy: RandomizationStrategy | "";
	blockSize: string;
	arms: Array<{ id: string; name: string; ratio: string }>;

	// Step 4: Keys & Strata
	keys: Array<{ id: string; name: string; type: KeyType | "" }>;
	strata: Array<{
		id: string;
		name: string;
		type: "fhir-path" | "custom" | "";
		values: string[];
	}>;
}

/** Configuration for a single wizard step. */
export interface StepConfig {
	id: number;
	label: string;
	shortLabel: string;
	validate: (data: CreateStudyFormData) => Record<string, string>;
}

/** Props passed to each step component. */
export interface StepProps {
	data: CreateStudyFormData;
	errors: Record<string, string>;
	onChange: (field: string, value: unknown) => void;
}

/** Reducer actions for mutating the wizard form state. */
export type FormAction =
	| { type: "UPDATE_FIELD"; field: string; value: unknown }
	| { type: "UPDATE_FIELDS"; fields: Partial<CreateStudyFormData> }
	| { type: "ADD_ARM" }
	| { type: "REMOVE_ARM"; id: string }
	| { type: "UPDATE_ARM"; id: string; field: string; value: string }
	| { type: "ADD_KEY" }
	| { type: "REMOVE_KEY"; id: string }
	| { type: "UPDATE_KEY"; id: string; field: string; value: string }
	| { type: "ADD_STRATUM" }
	| { type: "REMOVE_STRATUM"; id: string }
	| { type: "UPDATE_STRATUM"; id: string; field: string; value: string }
	| { type: "ADD_STRATUM_VALUE"; id: string; value: string }
	| { type: "REMOVE_STRATUM_VALUE"; id: string; index: number }
	| { type: "ADD_SITE" }
	| { type: "REMOVE_SITE"; id: string }
	| { type: "UPDATE_SITE"; id: string; field: string; value: string }
	| { type: "RESET" };

/** Initial default values for the form. */
export const INITIAL_FORM_DATA: CreateStudyFormData = {
	title: "",
	shortName: "",
	irbNumber: "",
	irbException: "none",
	protocolId: "",
	protocolLink: "",
	description: "",

	piName: "",
	piEmail: "",
	coordinatorName: "",
	coordinatorEmail: "",
	statisticianName: "",
	statisticianEmail: "",
	sites: [],
	startDate: "",
	endDate: "",
	targetEnrollment: "",

	strategy: "",
	blockSize: "",
	arms: [
		{ id: "arm-1", name: "", ratio: "1" },
		{ id: "arm-2", name: "", ratio: "1" },
	],

	keys: [{ id: "key-1", name: "", type: "" }],
	strata: [],
};

/** Step labels for the wizard stepper. */
export const STEPS: StepConfig[] = [
	{
		id: 1,
		label: "Study Information",
		shortLabel: "Study Info",
		validate: () => ({}),
	},
	{
		id: 2,
		label: "Team & Timeline",
		shortLabel: "Team",
		validate: () => ({}),
	},
	{
		id: 3,
		label: "Randomization Design",
		shortLabel: "Randomize",
		validate: () => ({}),
	},
	{
		id: 4,
		label: "Keys & Strata",
		shortLabel: "Keys",
		validate: () => ({}),
	},
	{
		id: 5,
		label: "Review & Confirm",
		shortLabel: "Review",
		validate: () => ({}),
	},
];
