import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { ArrowLeft, CheckCircle2, Loader2 } from "lucide-react";
import { useReducer, useState } from "react";
import type { CreateStudyRequest } from "#/api/client";
import { useCreateStudy } from "#/api/queries";
import {
	type CreateStudyFormData,
	type FormAction,
	INITIAL_FORM_DATA,
	STEPS,
	StepKeysStrata,
	StepRandomizationDesign,
	StepReview,
	StepStudyInfo,
	StepTeamTimeline,
	validateStep1,
	validateStep2,
	validateStep3,
	validateStep4,
	WizardStepper,
} from "#/components/studies/create-wizard";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/studies/new")({
	validateSearch: (search: Record<string, unknown>) => ({
		step: Math.min(5, Math.max(1, Number(search.step) || 1)),
	}),
	component: CreateStudyPage,
});

const VALIDATORS = [validateStep1, validateStep2, validateStep3, validateStep4];

// The wizard does not yet collect a FHIR context per stratum; patient-level
// stratification is the only supported case, so default to the Patient context.
const DEFAULT_STRATUM_CONTEXT = "Patient";

function formReducer(
	state: CreateStudyFormData,
	action: FormAction,
): CreateStudyFormData {
	switch (action.type) {
		case "UPDATE_FIELD":
			return { ...state, [action.field]: action.value };

		case "UPDATE_FIELDS":
			return { ...state, ...action.fields };

		case "ADD_ARM":
			return {
				...state,
				arms: [
					...state.arms,
					{ id: `arm-${Date.now()}`, name: "", ratio: "1" },
				],
			};
		case "REMOVE_ARM":
			return {
				...state,
				arms: state.arms.filter((a) => a.id !== action.id),
			};
		case "UPDATE_ARM":
			return {
				...state,
				arms: state.arms.map((a) =>
					a.id === action.id ? { ...a, [action.field]: action.value } : a,
				),
			};

		case "ADD_KEY":
			return {
				...state,
				keys: [...state.keys, { id: `key-${Date.now()}`, name: "", type: "" }],
			};
		case "REMOVE_KEY":
			return {
				...state,
				keys: state.keys.filter((k) => k.id !== action.id),
			};
		case "UPDATE_KEY":
			return {
				...state,
				keys: state.keys.map((k) =>
					k.id === action.id ? { ...k, [action.field]: action.value } : k,
				),
			};

		case "ADD_STRATUM":
			return {
				...state,
				strata: [
					...state.strata,
					{
						id: `stratum-${Date.now()}`,
						name: "",
						type: "",
						values: [],
					},
				],
			};
		case "REMOVE_STRATUM":
			return {
				...state,
				strata: state.strata.filter((s) => s.id !== action.id),
			};
		case "UPDATE_STRATUM":
			return {
				...state,
				strata: state.strata.map((s) =>
					s.id === action.id ? { ...s, [action.field]: action.value } : s,
				),
			};
		case "ADD_STRATUM_VALUE":
			return {
				...state,
				strata: state.strata.map((s) =>
					s.id === action.id
						? { ...s, values: [...s.values, action.value] }
						: s,
				),
			};
		case "REMOVE_STRATUM_VALUE":
			return {
				...state,
				strata: state.strata.map((s) =>
					s.id === action.id
						? {
								...s,
								values: s.values.filter((_, i) => i !== action.index),
							}
						: s,
				),
			};

		case "ADD_SITE":
			return {
				...state,
				sites: [
					...state.sites,
					{ id: `site-${Date.now()}`, name: "", enrollmentCap: "" },
				],
			};
		case "REMOVE_SITE":
			return {
				...state,
				sites: state.sites.filter((s) => s.id !== action.id),
			};
		case "UPDATE_SITE":
			return {
				...state,
				sites: state.sites.map((s) =>
					s.id === action.id ? { ...s, [action.field]: action.value } : s,
				),
			};

		case "RESET":
			return INITIAL_FORM_DATA;
	}
}

function CreateStudyPage() {
	const { step: currentStep } = Route.useSearch();
	const navigate = Route.useNavigate();
	const globalNavigate = useNavigate();
	const createStudy = useCreateStudy();

	const [formData, dispatch] = useReducer(formReducer, INITIAL_FORM_DATA);
	const [errors, setErrors] = useState<Record<string, string>>({});
	const [maxReachedStep, setMaxReachedStep] = useState(1);
	const [direction, setDirection] = useState<"forward" | "backward">("forward");
	const [isCreated, setIsCreated] = useState(false);

	function handleChange(field: string, value: unknown) {
		dispatch({ type: "UPDATE_FIELD", field, value });
		if (errors[field]) {
			setErrors((prev) => {
				const next = { ...prev };
				delete next[field];
				return next;
			});
		}
	}

	function handleNext() {
		if (currentStep >= 5) return;

		const validator = VALIDATORS[currentStep - 1];
		if (validator) {
			const stepErrors = validator(formData);
			if (Object.keys(stepErrors).length > 0) {
				setErrors(stepErrors);
				return;
			}
		}

		setErrors({});
		setDirection("forward");
		const nextStep = currentStep + 1;
		setMaxReachedStep((prev) => Math.max(prev, nextStep));
		navigate({ search: { step: nextStep } });
	}

	function handleBack() {
		if (currentStep <= 1) return;
		setErrors({});
		setDirection("backward");
		navigate({ search: { step: currentStep - 1 } });
	}

	function handleStepClick(targetStep: number) {
		if (targetStep === currentStep) return;
		if (targetStep > maxReachedStep + 1) return;

		setErrors({});
		setDirection(targetStep > currentStep ? "forward" : "backward");
		navigate({ search: { step: targetStep } });
	}

	function handleEditStep(targetStep: number) {
		setErrors({});
		setDirection("backward");
		navigate({ search: { step: targetStep } });
	}

	function handleCreate() {
		const KEY_TYPE_MAP: Record<
			string,
			"patientId" | "encounterId" | "providerId"
		> = {
			"patient-id": "patientId",
			"encounter-id": "encounterId",
			"provider-id": "providerId",
		};

		const request: CreateStudyRequest = {
			name: formData.title,
			description: formData.description,
			strategy: formData.strategy === "simple" ? "simple" : "block",
			arms: formData.arms
				.filter((a) => a.name.trim())
				.map((a) => ({ name: a.name, ratio: Number(a.ratio) || 1 })),
			keys: formData.keys
				.filter((k) => k.type)
				.map((k) => KEY_TYPE_MAP[k.type] ?? null),
			strata: formData.strata
				.filter((s) => s.name.trim())
				.map((s) => ({
					name: s.name,
					context: DEFAULT_STRATUM_CONTEXT,
					path: s.name.toLowerCase().replace(/\s+/g, "_"),
				})),
			blockSize: formData.blockSize ? Number(formData.blockSize) : undefined,
		};

		createStudy.mutate(request, {
			onSuccess: (study) => {
				setIsCreated(true);
				setTimeout(() => {
					globalNavigate({
						to: "/studies/$studyId",
						params: { studyId: study.id },
					});
				}, 1500);
			},
			onError: (error) => {
				setErrors({ form: error.message });
			},
		});
	}

	if (isCreated) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<CheckCircle2 className="size-12 text-primary" />
				<h2 className="text-xl font-semibold text-foreground">
					Study Created Successfully
				</h2>
				<p className="text-sm text-muted-foreground">
					Your study configuration has been saved.
				</p>
				<Button variant="outline" render={<Link to="/studies" />}>
					Back to Studies
				</Button>
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-6">
			<div className="flex flex-col gap-4">
				<Link
					to="/studies"
					className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
				>
					<ArrowLeft className="size-3.5" />
					Back to Studies
				</Link>

				<div>
					<h1 className="text-2xl font-semibold tracking-tight text-foreground">
						Create New Study
					</h1>
					<p className="text-sm text-muted-foreground">
						Define a new randomization study with its configuration.
					</p>
				</div>
			</div>

			<WizardStepper
				steps={STEPS}
				currentStep={currentStep}
				maxReachedStep={maxReachedStep}
				onStepClick={handleStepClick}
			/>

			<form
				className="flex flex-col gap-8"
				onSubmit={(e) => e.preventDefault()}
			>
				<div
					key={currentStep}
					className={
						direction === "forward"
							? "animate-in fade-in slide-in-from-right-4 duration-200"
							: "animate-in fade-in slide-in-from-left-4 duration-200"
					}
				>
					<ActiveStep
						step={currentStep}
						data={formData}
						errors={errors}
						onChange={handleChange}
						dispatch={dispatch}
						onEditStep={handleEditStep}
					/>
				</div>

				{errors.form && (
					<p className="text-sm text-destructive">{errors.form}</p>
				)}

				<div className="flex items-center gap-3">
					{currentStep > 1 && (
						<Button type="button" variant="outline" onClick={handleBack}>
							Back
						</Button>
					)}

					{currentStep < 5 ? (
						<Button type="button" onClick={handleNext}>
							Continue
						</Button>
					) : (
						<Button
							type="button"
							onClick={handleCreate}
							disabled={createStudy.isPending}
						>
							{createStudy.isPending && (
								<Loader2 className="size-4 animate-spin" />
							)}
							Create Study
						</Button>
					)}

					<Button variant="outline" render={<Link to="/studies" />}>
						Cancel
					</Button>
				</div>
			</form>
		</div>
	);
}

interface ActiveStepProps {
	step: number;
	data: CreateStudyFormData;
	errors: Record<string, string>;
	onChange: (field: string, value: unknown) => void;
	dispatch: React.Dispatch<FormAction>;
	onEditStep: (step: number) => void;
}

function ActiveStep({
	step,
	data,
	errors,
	onChange,
	dispatch,
	onEditStep,
}: ActiveStepProps) {
	switch (step) {
		case 1:
			return <StepStudyInfo data={data} errors={errors} onChange={onChange} />;
		case 2:
			return (
				<StepTeamTimeline
					data={data}
					errors={errors}
					onChange={onChange}
					onAddSite={() => dispatch({ type: "ADD_SITE" })}
					onRemoveSite={(id) => dispatch({ type: "REMOVE_SITE", id })}
					onUpdateSite={(id, field, value) =>
						dispatch({ type: "UPDATE_SITE", id, field, value })
					}
				/>
			);
		case 3:
			return (
				<StepRandomizationDesign
					data={data}
					errors={errors}
					onChange={onChange}
					onAddArm={() => dispatch({ type: "ADD_ARM" })}
					onRemoveArm={(id) => dispatch({ type: "REMOVE_ARM", id })}
					onUpdateArm={(id, field, value) =>
						dispatch({ type: "UPDATE_ARM", id, field, value })
					}
				/>
			);
		case 4:
			return (
				<StepKeysStrata
					data={data}
					errors={errors}
					onChange={onChange}
					onAddKey={() => dispatch({ type: "ADD_KEY" })}
					onRemoveKey={(id) => dispatch({ type: "REMOVE_KEY", id })}
					onUpdateKey={(id, field, value) =>
						dispatch({ type: "UPDATE_KEY", id, field, value })
					}
					onAddStratum={() => dispatch({ type: "ADD_STRATUM" })}
					onRemoveStratum={(id) => dispatch({ type: "REMOVE_STRATUM", id })}
					onUpdateStratum={(id, field, value) =>
						dispatch({ type: "UPDATE_STRATUM", id, field, value })
					}
					onAddStratumValue={(id, value) =>
						dispatch({ type: "ADD_STRATUM_VALUE", id, value })
					}
					onRemoveStratumValue={(id, index) =>
						dispatch({ type: "REMOVE_STRATUM_VALUE", id, index })
					}
				/>
			);
		case 5:
			return <StepReview data={data} onEditStep={onEditStep} />;
		default:
			return null;
	}
}
