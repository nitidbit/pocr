// Only strategies the engine implements. Block + strata is stratified block;
// there is no separate "stratified block" strategy.
export enum RandomizationStrategy {
	Simple = "simple",
	Block = "block",
}

// Mirrors the engine's KeyVariant. FhirPath is an advanced escape hatch.
export enum KeyType {
	PatientId = "patient-id",
	EncounterId = "encounter-id",
	ProviderId = "provider-id",
	FhirPath = "fhir-path",
}
