import type { CreateStudyFormData } from "./types";

/** Validates Step 1: Study Information. Title and IRB number are required. */
export function validateStep1(
	data: CreateStudyFormData,
): Record<string, string> {
	const errors: Record<string, string> = {};

	if (!data.title.trim()) {
		errors.title = "Study title is required";
	}

	if (!data.irbNumber.trim()) {
		errors.irbNumber = "IRB number is required";
	}

	return errors;
}

/** Validates Step 2: Team & Timeline. All fields are optional. */
export function validateStep2(
	_data: CreateStudyFormData,
): Record<string, string> {
	return {};
}

/** Validates Step 3: Randomization Design. Requires strategy, minimum 2 named arms. */
export function validateStep3(
	data: CreateStudyFormData,
): Record<string, string> {
	const errors: Record<string, string> = {};

	if (!data.strategy) {
		errors.strategy = "Randomization strategy is required";
	}

	if (data.arms.length < 2) {
		errors.arms = "At least 2 treatment arms are required";
	}

	for (let i = 0; i < data.arms.length; i++) {
		if (!data.arms[i].name.trim()) {
			errors[`arms.${i}.name`] = "Arm name is required";
		}
	}

	return errors;
}

/** Validates Step 4: Keys & Strata. Requires at least 1 key with name and type. */
export function validateStep4(
	data: CreateStudyFormData,
): Record<string, string> {
	const errors: Record<string, string> = {};

	if (data.keys.length < 1) {
		errors.keys = "At least 1 randomization key is required";
	}

	for (let i = 0; i < data.keys.length; i++) {
		if (!data.keys[i].name.trim()) {
			errors[`keys.${i}.name`] = "Key name is required";
		}
		if (!data.keys[i].type) {
			errors[`keys.${i}.type`] = "Key type is required";
		}
	}

	return errors;
}
