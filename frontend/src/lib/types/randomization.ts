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
