import { Plus, Trash2, X } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface StratumEditorProps {
	strata: Array<{
		id: string;
		name: string;
		type: "fhir-path" | "custom" | "";
		values: string[];
	}>;
	errors: Record<string, string>;
	onAdd: () => void;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
	onAddValue: (id: string, value: string) => void;
	onRemoveValue: (id: string, index: number) => void;
}

/** Dynamic list editor for stratification factors. */
export function StratumEditor({
	strata,
	errors,
	onAdd,
	onRemove,
	onUpdate,
	onAddValue,
	onRemoveValue,
}: StratumEditorProps) {
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
						onAddValue={onAddValue}
						onRemoveValue={onRemoveValue}
					/>
				))}
			</div>

			<Button
				type="button"
				variant="outline"
				size="sm"
				onClick={onAdd}
				className="self-start"
			>
				<Plus className="size-3.5" data-icon="inline-start" />
				Add Stratum
			</Button>
		</div>
	);
}

interface StratumRowProps {
	stratum: {
		id: string;
		name: string;
		type: "fhir-path" | "custom" | "";
		values: string[];
	};
	index: number;
	errors: Record<string, string>;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
	onAddValue: (id: string, value: string) => void;
	onRemoveValue: (id: string, index: number) => void;
}

function StratumRow({
	stratum,
	index,
	errors,
	onRemove,
	onUpdate,
	onAddValue,
	onRemoveValue,
}: StratumRowProps) {
	const [newValue, setNewValue] = useState("");

	function handleAddValue() {
		if (newValue.trim()) {
			onAddValue(stratum.id, newValue.trim());
			setNewValue("");
		}
	}

	return (
		<div className="flex flex-col gap-2 rounded-lg border bg-muted/30 p-3">
			<div className="flex items-start gap-2">
				<div className="flex flex-1 flex-col gap-1">
					<Input
						placeholder="Stratum name"
						value={stratum.name}
						onChange={(e) => onUpdate(stratum.id, "name", e.target.value)}
						aria-label={`Stratum ${index + 1} name`}
					/>
					{errors[`strata.${index}.name`] && (
						<p className="text-xs text-destructive">
							{errors[`strata.${index}.name`]}
						</p>
					)}
				</div>

				<div className="flex w-36 flex-col gap-1">
					<select
						className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm"
						value={stratum.type}
						onChange={(e) => onUpdate(stratum.id, "type", e.target.value)}
						aria-label={`Stratum ${index + 1} type`}
					>
						<option value="">Select type...</option>
						<option value="fhir-path">FHIR Path</option>
						<option value="custom">Custom</option>
					</select>
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

			{stratum.type === "custom" && (
				<div className="flex flex-col gap-2 pl-2">
					<div className="flex flex-wrap gap-1">
						{stratum.values.map((val, valIndex) => {
							const key = `${stratum.id}-val-${valIndex}`;
							return (
								<span
									key={key}
									className="inline-flex items-center gap-1 rounded-md bg-secondary px-2 py-0.5 text-xs text-secondary-foreground"
								>
									{val}
									<button
										type="button"
										onClick={() => onRemoveValue(stratum.id, valIndex)}
										className="rounded-sm hover:bg-destructive/20 hover:text-destructive"
										aria-label={`Remove value ${val}`}
									>
										<X className="size-3" />
									</button>
								</span>
							);
						})}
					</div>
					<div className="flex gap-1">
						<Input
							placeholder="Add value..."
							value={newValue}
							onChange={(e) => setNewValue(e.target.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter") {
									e.preventDefault();
									handleAddValue();
								}
							}}
							className="h-7 text-xs"
							aria-label={`New value for stratum ${index + 1}`}
						/>
						<Button
							type="button"
							variant="secondary"
							size="xs"
							onClick={handleAddValue}
						>
							Add
						</Button>
					</div>
				</div>
			)}
		</div>
	);
}
