import { Pencil } from "lucide-react";
import { KeyType, RandomizationStrategy } from "#/lib/types/randomization";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";

import type { CreateStudyFormData } from "./types";

interface StepReviewProps {
	data: CreateStudyFormData;
	onEditStep: (step: number) => void;
}

const STRATEGY_LABELS: Record<string, string> = {
	[RandomizationStrategy.Simple]: "Simple Randomization",
	[RandomizationStrategy.Block]: "Block Randomization",
	[RandomizationStrategy.StratifiedBlock]: "Stratified Block Randomization",
	[RandomizationStrategy.Minimization]: "Minimization",
};

const KEY_TYPE_LABELS: Record<string, string> = {
	[KeyType.PatientId]: "Patient ID",
	[KeyType.EncounterId]: "Encounter ID",
	[KeyType.ProviderId]: "Provider ID",
	[KeyType.ClusterId]: "Cluster ID",
	[KeyType.FhirPath]: "FHIR Path",
};

/** Step 5: Review & Confirm — read-only summary of all wizard data. */
export function StepReview({ data, onEditStep }: StepReviewProps) {
	return (
		<div className="flex flex-col gap-6">
			<div>
				<h2 className="text-lg font-semibold text-foreground">
					Review & Confirm
				</h2>
				<p className="text-sm text-muted-foreground">
					Review all study details before creating.
				</p>
			</div>

			<div className="flex flex-col gap-4">
				<ReviewCard title="Study Information" onEdit={() => onEditStep(1)}>
					<ReviewRow label="Title" value={data.title} />
					<ReviewRow label="Short Name" value={data.shortName} />
					<ReviewRow label="IRB Number" value={data.irbNumber} />
					<ReviewRow
						label="IRB Exception"
						value={data.irbException === "none" ? "None" : data.irbException}
					/>
					<ReviewRow label="Protocol ID" value={data.protocolId} />
					<ReviewRow label="Protocol Link" value={data.protocolLink} />
					{data.description && (
						<ReviewRow label="Description" value={data.description} />
					)}
				</ReviewCard>

				<ReviewCard title="Team & Timeline" onEdit={() => onEditStep(2)}>
					{data.piName && (
						<ReviewRow
							label="PI"
							value={`${data.piName}${data.piEmail ? ` (${data.piEmail})` : ""}`}
						/>
					)}
					{data.coordinatorName && (
						<ReviewRow
							label="Coordinator"
							value={`${data.coordinatorName}${data.coordinatorEmail ? ` (${data.coordinatorEmail})` : ""}`}
						/>
					)}
					{data.statisticianName && (
						<ReviewRow
							label="Statistician"
							value={`${data.statisticianName}${data.statisticianEmail ? ` (${data.statisticianEmail})` : ""}`}
						/>
					)}
					{data.sites.length > 0 && (
						<div className="flex flex-col gap-1">
							<span className="text-xs font-medium text-muted-foreground">
								Sites
							</span>
							<div className="flex flex-wrap gap-1">
								{data.sites.map((site) => (
									<Badge key={site.id} variant="secondary">
										{site.name || "Unnamed"}
										{site.enrollmentCap && ` (cap: ${site.enrollmentCap})`}
									</Badge>
								))}
							</div>
						</div>
					)}
					<div className="grid gap-2 sm:grid-cols-3">
						<ReviewRow label="Start Date" value={data.startDate} />
						<ReviewRow label="End Date" value={data.endDate} />
						<ReviewRow
							label="Target Enrollment"
							value={data.targetEnrollment}
						/>
					</div>
				</ReviewCard>

				<ReviewCard title="Randomization Design" onEdit={() => onEditStep(3)}>
					<ReviewRow
						label="Strategy"
						value={
							data.strategy
								? (STRATEGY_LABELS[data.strategy] ?? data.strategy)
								: ""
						}
					/>
					{data.blockSize && (
						<ReviewRow label="Block Size" value={data.blockSize} />
					)}
					{data.arms.length > 0 && (
						<div className="flex flex-col gap-1">
							<span className="text-xs font-medium text-muted-foreground">
								Arms
							</span>
							<div className="flex flex-wrap gap-1">
								{data.arms.map((arm) => (
									<Badge key={arm.id} variant="outline">
										{arm.name || "Unnamed"} ({arm.ratio}:1)
									</Badge>
								))}
							</div>
						</div>
					)}
				</ReviewCard>

				<ReviewCard title="Keys & Strata" onEdit={() => onEditStep(4)}>
					{data.keys.length > 0 && (
						<div className="flex flex-col gap-1">
							<span className="text-xs font-medium text-muted-foreground">
								Randomization Keys
							</span>
							<div className="flex flex-wrap gap-1">
								{data.keys.map((key) => (
									<Badge key={key.id} variant="outline">
										{key.name || "Unnamed"} (
										{key.type
											? (KEY_TYPE_LABELS[key.type] ?? key.type)
											: "unset"}
										)
									</Badge>
								))}
							</div>
						</div>
					)}
					{data.strata.length > 0 && (
						<>
							<Separator />
							<div className="flex flex-col gap-1">
								<span className="text-xs font-medium text-muted-foreground">
									Stratification Factors
								</span>
								<div className="flex flex-wrap gap-1">
									{data.strata.map((s) => (
										<Badge key={s.id} variant="secondary">
											{s.name || "Unnamed"} ({s.type || "unset"})
											{s.values.length > 0 && `: ${s.values.join(", ")}`}
										</Badge>
									))}
								</div>
							</div>
						</>
					)}
				</ReviewCard>
			</div>
		</div>
	);
}

interface ReviewCardProps {
	title: string;
	onEdit: () => void;
	children: React.ReactNode;
}

function ReviewCard({ title, onEdit, children }: ReviewCardProps) {
	return (
		<Card size="sm">
			<CardHeader className="flex-row items-center justify-between">
				<CardTitle>{title}</CardTitle>
				<Button type="button" variant="ghost" size="xs" onClick={onEdit}>
					<Pencil className="size-3" data-icon="inline-start" />
					Edit
				</Button>
			</CardHeader>
			<CardContent className="flex flex-col gap-2">{children}</CardContent>
		</Card>
	);
}

interface ReviewRowProps {
	label: string;
	value: string;
}

function ReviewRow({ label, value }: ReviewRowProps) {
	if (!value) return null;
	return (
		<div className="flex flex-col gap-0.5">
			<span className="text-xs font-medium text-muted-foreground">{label}</span>
			<span className="text-sm text-foreground">{value}</span>
		</div>
	);
}
