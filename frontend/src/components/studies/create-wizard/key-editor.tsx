import { Plus, Trash2 } from "lucide-react";
import { KeyType } from "#/lib/types/randomization";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface KeyEditorProps {
	keys: Array<{ id: string; name: string; type: string }>;
	errors: Record<string, string>;
	onAdd: () => void;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
}

const KEY_TYPE_OPTIONS = [
	{ value: KeyType.PatientId, label: "Patient ID" },
	{ value: KeyType.EncounterId, label: "Encounter ID" },
	{ value: KeyType.ProviderId, label: "Provider ID" },
	{ value: KeyType.ClusterId, label: "Cluster ID" },
	{ value: KeyType.FhirPath, label: "FHIR Path" },
];

/** Dynamic list editor for randomization keys with name and type fields. */
export function KeyEditor({
	keys,
	errors,
	onAdd,
	onRemove,
	onUpdate,
}: KeyEditorProps) {
	const canRemove = keys.length > 1;

	return (
		<div className="flex flex-col gap-3">
			<div className="flex flex-col gap-2">
				{keys.map((key, index) => (
					<div
						key={key.id}
						className="flex items-start gap-2 rounded-lg border bg-muted/30 p-3"
					>
						<div className="flex flex-1 flex-col gap-1">
							<Input
								placeholder="Key name"
								value={key.name}
								onChange={(e) => onUpdate(key.id, "name", e.target.value)}
								aria-label={`Key ${index + 1} name`}
								aria-invalid={!!errors[`keys.${index}.name`]}
							/>
							{errors[`keys.${index}.name`] && (
								<p className="text-xs text-destructive">
									{errors[`keys.${index}.name`]}
								</p>
							)}
						</div>

						<div className="flex w-40 flex-col gap-1">
							<select
								className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm"
								value={key.type}
								onChange={(e) => onUpdate(key.id, "type", e.target.value)}
								aria-label={`Key ${index + 1} type`}
								aria-invalid={!!errors[`keys.${index}.type`]}
							>
								<option value="">Select type...</option>
								{KEY_TYPE_OPTIONS.map((opt) => (
									<option key={opt.value} value={opt.value}>
										{opt.label}
									</option>
								))}
							</select>
							{errors[`keys.${index}.type`] && (
								<p className="text-xs text-destructive">
									{errors[`keys.${index}.type`]}
								</p>
							)}
						</div>

						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							disabled={!canRemove}
							onClick={() => onRemove(key.id)}
							aria-label={`Remove key ${index + 1}`}
						>
							<Trash2 className="size-3.5" />
						</Button>
					</div>
				))}
			</div>

			{errors.keys && <p className="text-xs text-destructive">{errors.keys}</p>}

			<Button
				type="button"
				variant="outline"
				size="sm"
				onClick={onAdd}
				className="self-start"
			>
				<Plus className="size-3.5" data-icon="inline-start" />
				Add Key
			</Button>
		</div>
	);
}
