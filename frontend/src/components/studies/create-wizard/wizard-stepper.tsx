import { Check } from "lucide-react";

import { cn } from "@/lib/utils";

import type { StepConfig } from "./types";

interface WizardStepperProps {
	steps: StepConfig[];
	currentStep: number;
	maxReachedStep: number;
	onStepClick: (step: number) => void;
}

/** Horizontal step indicator with completed/current/upcoming states. */
export function WizardStepper({
	steps,
	currentStep,
	maxReachedStep,
	onStepClick,
}: WizardStepperProps) {
	return (
		<nav aria-label="Wizard progress">
			<ol className="flex items-center gap-0">
				{steps.map((step, index) => {
					const isCompleted = step.id < currentStep;
					const isCurrent = step.id === currentStep;
					const isClickable = step.id <= maxReachedStep + 1;
					const isLast = index === steps.length - 1;

					return (
						<li
							key={step.id}
							className={cn("flex items-center", !isLast && "flex-1")}
						>
							<button
								type="button"
								aria-current={isCurrent ? "step" : undefined}
								aria-label={`Step ${step.id}: ${step.label}`}
								disabled={!isClickable}
								onClick={() => onStepClick(step.id)}
								className={cn(
									"group flex flex-col items-center gap-1.5 focus-visible:outline-none",
									isClickable &&
										"cursor-pointer focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 rounded-sm",
									!isClickable && "cursor-not-allowed opacity-50",
								)}
							>
								<span
									className={cn(
										"flex size-8 shrink-0 items-center justify-center rounded-full border-2 text-xs font-semibold transition-all",
										isCompleted &&
											"border-primary bg-primary text-primary-foreground",
										isCurrent &&
											"border-primary bg-primary/10 text-primary ring-4 ring-primary/20",
										!isCompleted &&
											!isCurrent &&
											"border-muted-foreground/30 text-muted-foreground",
									)}
								>
									{isCompleted ? (
										<Check className="size-4" strokeWidth={3} />
									) : (
										step.id
									)}
								</span>
								<span
									className={cn(
										"hidden text-xs sm:block",
										isCurrent && "font-semibold text-foreground",
										isCompleted && "font-medium text-foreground",
										!isCompleted && !isCurrent && "text-muted-foreground",
									)}
								>
									{step.shortLabel}
								</span>
							</button>

							{!isLast && (
								<div
									className={cn(
										"mx-2 h-0.5 flex-1 rounded-full transition-colors",
										isCompleted ? "bg-primary" : "bg-border",
									)}
									aria-hidden="true"
								/>
							)}
						</li>
					);
				})}
			</ol>
		</nav>
	);
}
