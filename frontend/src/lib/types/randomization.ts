export enum RandomizationStrategy {
	Simple = "simple",
	Block = "block",
	StratifiedBlock = "stratified-block",
	Minimization = "minimization",
}

export enum KeyType {
	PatientId = "patient-id",
	EncounterId = "encounter-id",
	ProviderId = "provider-id",
	ClusterId = "cluster-id",
	FhirPath = "fhir-path",
}

export interface Arm {
	id: string;
	name: string;
	ratio: number;
}

export interface Key {
	id: string;
	name: string;
	type: KeyType;
}

export interface Stratum {
	id: string;
	name: string;
	type: "fhir-path" | "custom";
	values?: string[];
}

export interface RandomizationConfig {
	strategy: RandomizationStrategy;
	arms: Arm[];
	keys: Key[];
	strata: Stratum[];
	blockSize?: number;
}
