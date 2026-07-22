import { Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import { STRATUM_PRESETS, type StratumPreset } from "./types";

interface Stratum {
	id: string;
	name: string;
	type: "fhir-path" | "";
	context: string;
	path: string;
}

interface StratumEditorProps {
	strata: Stratum[];
	errors: Record<string, string>;
	onAdd: () => void;
	onAddPreset: (preset: StratumPreset) => void;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
}

/** Editor for stratification factors. Offers standard FHIR presets (known
 * context + path) plus a custom FHIRPath escape hatch. */
export function StratumEditor({
	strata,
	errors,
	onAdd,
	onAddPreset,
	onRemove,
	onUpdate,
}: StratumEditorProps) {
	const usedPaths = new Set(strata.map((s) => `${s.context}:${s.path}`));

	return (
		<div className="flex flex-col gap-3">
			{strata.length === 0 && (
				<p className="text-sm text-muted-foreground">
					No stratification factors defined. Add one to balance allocation
					across subgroups.
				</p>
			)}

			<div className="flex flex-col gap-2">
				{strata.map((stratum, index) => (
					<StratumRow
						key={stratum.id}
						stratum={stratum}
						index={index}
						errors={errors}
						onRemove={onRemove}
						onUpdate={onUpdate}
					/>
				))}
			</div>

			<div className="flex flex-wrap items-center gap-2">
				{STRATUM_PRESETS.map(({ label, preset }) => (
					<Button
						key={label}
						type="button"
						variant="outline"
						size="sm"
						disabled={usedPaths.has(`${preset.context}:${preset.path}`)}
						onClick={() => onAddPreset(preset)}
					>
						<Plus className="size-3.5" data-icon="inline-start" />
						{label}
					</Button>
				))}
				<Button type="button" variant="outline" size="sm" onClick={onAdd}>
					<Plus className="size-3.5" data-icon="inline-start" />
					Custom FHIRPath
				</Button>
			</div>
		</div>
	);
}

interface StratumRowProps {
	stratum: Stratum;
	index: number;
	errors: Record<string, string>;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
}

function StratumRow({
	stratum,
	index,
	errors,
	onRemove,
	onUpdate,
}: StratumRowProps) {
	return (
		<div className="flex items-start gap-2 rounded-lg border bg-muted/30 p-3">
			<div className="flex flex-1 flex-col gap-1">
				<Input
					placeholder="Label (e.g. Sex)"
					value={stratum.name}
					onChange={(e) => onUpdate(stratum.id, "name", e.target.value)}
					aria-label={`Stratum ${index + 1} label`}
					aria-invalid={!!errors[`strata.${index}.name`]}
				/>
				{errors[`strata.${index}.name`] && (
					<p className="text-xs text-destructive">
						{errors[`strata.${index}.name`]}
					</p>
				)}
			</div>

			<div className="flex w-28 flex-col gap-1">
				<Input
					placeholder="Context"
					value={stratum.context}
					onChange={(e) => onUpdate(stratum.id, "context", e.target.value)}
					aria-label={`Stratum ${index + 1} FHIR context`}
				/>
			</div>

			<div className="flex flex-1 flex-col gap-1">
				<Input
					placeholder="FHIRPath (e.g. gender)"
					value={stratum.path}
					onChange={(e) => onUpdate(stratum.id, "path", e.target.value)}
					aria-label={`Stratum ${index + 1} FHIRPath`}
					aria-invalid={!!errors[`strata.${index}.path`]}
				/>
				{errors[`strata.${index}.path`] && (
					<p className="text-xs text-destructive">
						{errors[`strata.${index}.path`]}
					</p>
				)}
			</div>

			<Button
				type="button"
				variant="ghost"
				size="icon-sm"
				onClick={() => onRemove(stratum.id)}
				aria-label={`Remove stratum ${index + 1}`}
			>
				<Trash2 className="size-3.5" />
			</Button>
		</div>
	);
}
