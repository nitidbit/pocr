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
	keys: Array<{ id: string; type: KeyType | "" }>;
	strata: Array<{
		id: string;
		name: string;
		// "fhir-path" is an advanced raw FHIRPath; presets set name + path directly.
		type: "fhir-path" | "";
		context: string;
		path: string;
	}>;
}

/** Configuration for a single wizard step. */
export interface StepConfig {
	id: number;
	label: string;
	shortLabel: string;
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
	| {
			type: "ADD_STRATUM_PRESET";
			preset: {
				name: string;
				type: "fhir-path" | "";
				context: string;
				path: string;
			};
	  }
	| { type: "REMOVE_STRATUM"; id: string }
	| { type: "UPDATE_STRATUM"; id: string; field: string; value: string }
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
		{ id: "arm-1", name: "Control", ratio: "1" },
		{ id: "arm-2", name: "Treatment", ratio: "1" },
	],

	keys: [{ id: "key-1", type: "" }],
	strata: [],
};

/** Sensible default name for a newly added arm at position `index` (0-based). */
export function defaultArmName(index: number): string {
	if (index === 0) return "Control";
	if (index === 1) return "Treatment";
	return `Arm ${index + 1}`;
}

/** A stratum the wizard can add. Presets carry a known FHIR context + path we
 * know how to resolve at randomization time. */
export interface StratumPreset {
	name: string;
	type: "fhir-path" | "";
	context: string;
	path: string;
}

/** Standard FHIR-derived stratification factors offered as one-click presets. */
export const STRATUM_PRESETS: Array<{ label: string; preset: StratumPreset }> =
	[
		{
			label: "Sex / Gender",
			preset: { name: "Sex", type: "", context: "Patient", path: "gender" },
		},
	];

/** Step labels for the wizard stepper. */
export const STEPS: StepConfig[] = [
	{ id: 1, label: "Study Information", shortLabel: "Study Info" },
	{ id: 2, label: "Team & Timeline", shortLabel: "Team" },
	{ id: 3, label: "Randomization Design", shortLabel: "Randomize" },
	{ id: 4, label: "Keys & Strata", shortLabel: "Keys" },
	{ id: 5, label: "Review & Confirm", shortLabel: "Review" },
];
