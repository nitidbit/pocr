import { Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";

import { FormField } from "./form-field";
import type { CreateStudyFormData, StepProps } from "./types";

interface StepTeamTimelineProps extends StepProps {
	onAddSite: () => void;
	onRemoveSite: (id: string) => void;
	onUpdateSite: (id: string, field: string, value: string) => void;
}

/** Step 2: Team & Timeline — PI, coordinator, statistician, sites, dates. */
export function StepTeamTimeline({
	data,
	errors,
	onChange,
	onAddSite,
	onRemoveSite,
	onUpdateSite,
}: StepTeamTimelineProps) {
	return (
		<div className="flex flex-col gap-6">
			<div>
				<h2 className="text-lg font-semibold text-foreground">
					Study Team & Timeline
				</h2>
				<p className="text-sm text-muted-foreground">
					Key personnel and scheduling information.
				</p>
			</div>

			<fieldset className="flex flex-col gap-4">
				<legend className="text-sm font-medium text-foreground">
					Principal Investigator
				</legend>
				<div className="grid gap-4 sm:grid-cols-2">
					<FormField label="Name" name="piName" error={errors.piName}>
						<Input
							placeholder="PI name"
							value={data.piName}
							onChange={(e) => onChange("piName", e.target.value)}
						/>
					</FormField>
					<FormField label="Email" name="piEmail" error={errors.piEmail}>
						<Input
							type="email"
							placeholder="pi@ucsf.edu"
							value={data.piEmail}
							onChange={(e) => onChange("piEmail", e.target.value)}
						/>
					</FormField>
				</div>
			</fieldset>

			<fieldset className="flex flex-col gap-4">
				<legend className="text-sm font-medium text-foreground">
					Study Coordinator
				</legend>
				<div className="grid gap-4 sm:grid-cols-2">
					<FormField
						label="Name"
						name="coordinatorName"
						error={errors.coordinatorName}
					>
						<Input
							placeholder="Coordinator name"
							value={data.coordinatorName}
							onChange={(e) => onChange("coordinatorName", e.target.value)}
						/>
					</FormField>
					<FormField
						label="Email"
						name="coordinatorEmail"
						error={errors.coordinatorEmail}
					>
						<Input
							type="email"
							placeholder="coordinator@ucsf.edu"
							value={data.coordinatorEmail}
							onChange={(e) => onChange("coordinatorEmail", e.target.value)}
						/>
					</FormField>
				</div>
			</fieldset>

			<fieldset className="flex flex-col gap-4">
				<legend className="text-sm font-medium text-foreground">
					Statistician
				</legend>
				<div className="grid gap-4 sm:grid-cols-2">
					<FormField
						label="Name"
						name="statisticianName"
						error={errors.statisticianName}
					>
						<Input
							placeholder="Statistician name"
							value={data.statisticianName}
							onChange={(e) => onChange("statisticianName", e.target.value)}
						/>
					</FormField>
					<FormField
						label="Email"
						name="statisticianEmail"
						error={errors.statisticianEmail}
					>
						<Input
							type="email"
							placeholder="stats@ucsf.edu"
							value={data.statisticianEmail}
							onChange={(e) => onChange("statisticianEmail", e.target.value)}
						/>
					</FormField>
				</div>
			</fieldset>

			<Separator />

			<SitesSection
				sites={data.sites}
				onAdd={onAddSite}
				onRemove={onRemoveSite}
				onUpdate={onUpdateSite}
			/>

			<Separator />

			<div className="grid gap-4 sm:grid-cols-3">
				<FormField label="Start Date" name="startDate" error={errors.startDate}>
					<Input
						type="date"
						value={data.startDate}
						onChange={(e) => onChange("startDate", e.target.value)}
					/>
				</FormField>
				<FormField label="End Date" name="endDate" error={errors.endDate}>
					<Input
						type="date"
						value={data.endDate}
						onChange={(e) => onChange("endDate", e.target.value)}
					/>
				</FormField>
				<FormField
					label="Target Enrollment"
					name="targetEnrollment"
					error={errors.targetEnrollment}
				>
					<Input
						type="number"
						min={1}
						placeholder="e.g. 200"
						value={data.targetEnrollment}
						onChange={(e) => onChange("targetEnrollment", e.target.value)}
					/>
				</FormField>
			</div>
		</div>
	);
}

interface SitesSectionProps {
	sites: CreateStudyFormData["sites"];
	onAdd: () => void;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
}

function SitesSection({ sites, onAdd, onRemove, onUpdate }: SitesSectionProps) {
	return (
		<div className="flex flex-col gap-3">
			<div className="flex items-center justify-between">
				<h3 className="text-sm font-medium text-foreground">Study Sites</h3>
				<Button type="button" variant="outline" size="xs" onClick={onAdd}>
					<Plus className="size-3" data-icon="inline-start" />
					Add Site
				</Button>
			</div>

			{sites.length === 0 && (
				<p className="text-sm text-muted-foreground">
					No sites added. Add at least one site for multi-center studies.
				</p>
			)}

			{sites.map((site, index) => (
				<div
					key={site.id}
					className="flex items-start gap-2 rounded-lg border bg-muted/30 p-3"
				>
					<div className="flex flex-1 gap-2">
						<Input
							placeholder="Site name"
							value={site.name}
							onChange={(e) => onUpdate(site.id, "name", e.target.value)}
							aria-label={`Site ${index + 1} name`}
						/>
						<Input
							type="number"
							min={0}
							placeholder="Cap"
							value={site.enrollmentCap}
							onChange={(e) =>
								onUpdate(site.id, "enrollmentCap", e.target.value)
							}
							className="w-24"
							aria-label={`Site ${index + 1} enrollment cap`}
						/>
					</div>
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						onClick={() => onRemove(site.id)}
						aria-label={`Remove site ${index + 1}`}
					>
						<Trash2 className="size-3.5" />
					</Button>
				</div>
			))}
		</div>
	);
}
