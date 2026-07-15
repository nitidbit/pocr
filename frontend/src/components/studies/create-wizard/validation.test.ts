import { describe, expect, it } from "vitest";
import { KeyType, RandomizationStrategy } from "#/lib/types/randomization";
import type { CreateStudyFormData } from "./types";
import {
	validateStep1,
	validateStep2,
	validateStep3,
	validateStep4,
} from "./validation";

function emptyFormData(): CreateStudyFormData {
	return {
		// Step 1
		title: "",
		shortName: "",
		irbNumber: "",
		irbException: "none",
		protocolId: "",
		protocolLink: "",
		description: "",

		// Step 2
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

		// Step 3
		strategy: "",
		blockSize: "",
		arms: [],

		// Step 4
		keys: [],
		strata: [],
	};
}

describe("validateStep1", () => {
	it("returns error for title when empty", () => {
		const data = emptyFormData();

		const errors = validateStep1(data);

		expect(errors.title).toBeDefined();
	});

	it("returns error for irbNumber when empty", () => {
		const data = emptyFormData();

		const errors = validateStep1(data);

		expect(errors.irbNumber).toBeDefined();
	});

	it("returns no errors when title and irbNumber are filled", () => {
		const data = emptyFormData();
		data.title = "Phase III Oncology Trial";
		data.irbNumber = "IRB-2026-0042";

		const errors = validateStep1(data);

		expect(Object.keys(errors)).toHaveLength(0);
	});

	it("does not require optional fields (shortName, protocolId, protocolLink, description)", () => {
		const data = emptyFormData();
		data.title = "A study";
		data.irbNumber = "IRB-001";

		const errors = validateStep1(data);

		expect(errors.shortName).toBeUndefined();
		expect(errors.protocolId).toBeUndefined();
		expect(errors.protocolLink).toBeUndefined();
		expect(errors.description).toBeUndefined();
	});

	it("trims whitespace-only title as empty", () => {
		const data = emptyFormData();
		data.title = "   ";
		data.irbNumber = "IRB-001";

		const errors = validateStep1(data);

		expect(errors.title).toBeDefined();
	});
});

describe("validateStep2 (all fields optional — no required field validation)", () => {
	it("returns no errors when all fields are empty", () => {
		const data = emptyFormData();

		const errors = validateStep2(data);

		expect(Object.keys(errors)).toHaveLength(0);
	});
});

describe("validateStep3", () => {
	it("returns error when fewer than 2 arms", () => {
		const data = emptyFormData();
		data.strategy = RandomizationStrategy.Simple;
		data.arms = [{ id: "1", name: "Control", ratio: "1" }];

		const errors = validateStep3(data);

		expect(errors.arms).toBeDefined();
	});

	it("returns error when arms have empty names", () => {
		const data = emptyFormData();
		data.strategy = RandomizationStrategy.Simple;
		data.arms = [
			{ id: "1", name: "", ratio: "1" },
			{ id: "2", name: "Treatment", ratio: "1" },
		];

		const errors = validateStep3(data);

		expect(errors["arms.0.name"]).toBeDefined();
	});

	it("returns error when strategy is not selected", () => {
		const data = emptyFormData();
		data.arms = [
			{ id: "1", name: "Control", ratio: "1" },
			{ id: "2", name: "Treatment", ratio: "1" },
		];

		const errors = validateStep3(data);

		expect(errors.strategy).toBeDefined();
	});

	it("returns no errors when strategy is set and 2+ named arms exist", () => {
		const data = emptyFormData();
		data.strategy = RandomizationStrategy.Block;
		data.arms = [
			{ id: "1", name: "Control", ratio: "1" },
			{ id: "2", name: "Treatment", ratio: "1" },
		];

		const errors = validateStep3(data);

		expect(Object.keys(errors)).toHaveLength(0);
	});

	it("returns error when all arms have empty names", () => {
		const data = emptyFormData();
		data.strategy = RandomizationStrategy.Simple;
		data.arms = [
			{ id: "1", name: "", ratio: "1" },
			{ id: "2", name: "", ratio: "1" },
		];

		const errors = validateStep3(data);

		expect(errors["arms.0.name"]).toBeDefined();
		expect(errors["arms.1.name"]).toBeDefined();
	});
});

describe("validateStep4", () => {
	it("returns error when no keys are defined", () => {
		const data = emptyFormData();

		const errors = validateStep4(data);

		expect(errors.keys).toBeDefined();
	});

	it("returns error when a key has no name", () => {
		const data = emptyFormData();
		data.keys = [{ id: "1", name: "", type: KeyType.PatientId }];

		const errors = validateStep4(data);

		expect(errors["keys.0.name"]).toBeDefined();
	});

	it("returns error when a key has no type", () => {
		const data = emptyFormData();
		data.keys = [{ id: "1", name: "Patient ID", type: "" }];

		const errors = validateStep4(data);

		expect(errors["keys.0.type"]).toBeDefined();
	});

	it("returns no errors when at least 1 key has name and type", () => {
		const data = emptyFormData();
		data.keys = [{ id: "1", name: "Patient ID", type: KeyType.PatientId }];

		const errors = validateStep4(data);

		expect(Object.keys(errors)).toHaveLength(0);
	});

	it("does not require strata (optional)", () => {
		const data = emptyFormData();
		data.keys = [{ id: "1", name: "Patient ID", type: KeyType.PatientId }];
		data.strata = [];

		const errors = validateStep4(data);

		expect(errors.strata).toBeUndefined();
	});

	it("validates each key independently", () => {
		const data = emptyFormData();
		data.keys = [
			{ id: "1", name: "Patient ID", type: KeyType.PatientId },
			{ id: "2", name: "", type: "" },
		];

		const errors = validateStep4(data);

		expect(errors["keys.0.name"]).toBeUndefined();
		expect(errors["keys.0.type"]).toBeUndefined();
		expect(errors["keys.1.name"]).toBeDefined();
		expect(errors["keys.1.type"]).toBeDefined();
	});
});
