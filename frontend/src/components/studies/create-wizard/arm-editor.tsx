import { Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface ArmEditorProps {
	arms: Array<{ id: string; name: string; ratio: string }>;
	errors: Record<string, string>;
	onAdd: () => void;
	onRemove: (id: string) => void;
	onUpdate: (id: string, field: string, value: string) => void;
}

/** Dynamic list editor for treatment arms with name and ratio fields. */
export function ArmEditor({
	arms,
	errors,
	onAdd,
	onRemove,
	onUpdate,
}: ArmEditorProps) {
	const canRemove = arms.length > 2;

	return (
		<div className="flex flex-col gap-3">
			<div className="flex flex-col gap-2">
				{arms.map((arm, index) => (
					<div
						key={arm.id}
						className="flex items-start gap-2 rounded-lg border bg-muted/30 p-3"
					>
						<div className="flex flex-1 flex-col gap-1">
							<Input
								placeholder={`Arm ${index + 1} name`}
								value={arm.name}
								onChange={(e) => onUpdate(arm.id, "name", e.target.value)}
								aria-label={`Arm ${index + 1} name`}
								aria-invalid={!!errors[`arms.${index}.name`]}
							/>
							{errors[`arms.${index}.name`] && (
								<p className="text-xs text-destructive">
									{errors[`arms.${index}.name`]}
								</p>
							)}
						</div>

						<div className="flex w-20 flex-col gap-1">
							<Input
								type="number"
								min={1}
								placeholder="1"
								value={arm.ratio}
								onChange={(e) => onUpdate(arm.id, "ratio", e.target.value)}
								aria-label={`Arm ${index + 1} ratio`}
							/>
						</div>

						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							disabled={!canRemove}
							onClick={() => onRemove(arm.id)}
							aria-label={`Remove arm ${index + 1}`}
						>
							<Trash2 className="size-3.5" />
						</Button>
					</div>
				))}
			</div>

			{errors.arms && <p className="text-xs text-destructive">{errors.arms}</p>}

			<Button
				type="button"
				variant="outline"
				size="sm"
				onClick={onAdd}
				className="self-start"
			>
				<Plus className="size-3.5" data-icon="inline-start" />
				Add Arm
			</Button>
		</div>
	);
}
