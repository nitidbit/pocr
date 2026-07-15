import { Input } from "@/components/ui/input";

import { FormField } from "./form-field";
import type { StepProps } from "./types";

/** Step 1: Study Information — title, IRB, protocol details. */
export function StepStudyInfo({ data, errors, onChange }: StepProps) {
	return (
		<div className="flex flex-col gap-6">
			<div>
				<h2 className="text-lg font-semibold text-foreground">
					Study Information
				</h2>
				<p className="text-sm text-muted-foreground">
					Basic details about the clinical trial.
				</p>
			</div>

			<div className="grid gap-4 sm:grid-cols-2">
				<FormField
					label="Study Title"
					name="title"
					required
					error={errors.title}
				>
					<Input
						placeholder="Full study title"
						value={data.title}
						onChange={(e) => onChange("title", e.target.value)}
					/>
				</FormField>

				<FormField label="Short Name" name="shortName" error={errors.shortName}>
					<Input
						placeholder="e.g. FLUID-CARDIAC"
						value={data.shortName}
						onChange={(e) => onChange("shortName", e.target.value)}
					/>
				</FormField>
			</div>

			<div className="grid gap-4 sm:grid-cols-2">
				<FormField
					label="IRB Number"
					name="irbNumber"
					required
					error={errors.irbNumber}
				>
					<Input
						placeholder="e.g. IRB-2024-0142"
						value={data.irbNumber}
						onChange={(e) => onChange("irbNumber", e.target.value)}
					/>
				</FormField>

				<FormField
					label="IRB Exception"
					name="irbException"
					error={errors.irbException}
				>
					<select
						className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm"
						value={data.irbException}
						onChange={(e) => onChange("irbException", e.target.value)}
					>
						<option value="none">None</option>
						<option value="waiver">Waiver</option>
						<option value="exempt">Exempt</option>
					</select>
				</FormField>
			</div>

			<div className="grid gap-4 sm:grid-cols-2">
				<FormField
					label="Protocol ID"
					name="protocolId"
					error={errors.protocolId}
				>
					<Input
						placeholder="e.g. UCSF-CS-2024-01"
						value={data.protocolId}
						onChange={(e) => onChange("protocolId", e.target.value)}
					/>
				</FormField>

				<FormField
					label="Protocol Link"
					name="protocolLink"
					error={errors.protocolLink}
				>
					<Input
						type="url"
						placeholder="https://..."
						value={data.protocolLink}
						onChange={(e) => onChange("protocolLink", e.target.value)}
					/>
				</FormField>
			</div>

			<FormField
				label="Description"
				name="description"
				error={errors.description}
			>
				<textarea
					className="min-h-24 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
					placeholder="Describe the study objectives, population, and design..."
					rows={4}
					value={data.description}
					onChange={(e) => onChange("description", e.target.value)}
				/>
			</FormField>
		</div>
	);
}
