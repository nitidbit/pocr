import { Separator } from "@/components/ui/separator";

import { KeyEditor } from "./key-editor";
import { StratumEditor } from "./stratum-editor";
import type { StepProps, StratumPreset } from "./types";

interface StepKeysStrataProps extends StepProps {
	onAddKey: () => void;
	onRemoveKey: (id: string) => void;
	onUpdateKey: (id: string, field: string, value: string) => void;
	onAddStratum: () => void;
	onAddStratumPreset: (preset: StratumPreset) => void;
	onRemoveStratum: (id: string) => void;
	onUpdateStratum: (id: string, field: string, value: string) => void;
}

/** Step 4: Keys & Strata — randomization unit keys and stratification factors. */
export function StepKeysStrata({
	data,
	errors,
	onAddKey,
	onRemoveKey,
	onUpdateKey,
	onAddStratum,
	onAddStratumPreset,
	onRemoveStratum,
	onUpdateStratum,
}: StepKeysStrataProps) {
	return (
		<div className="flex flex-col gap-6">
			<div>
				<h2 className="text-lg font-semibold text-foreground">Keys & Strata</h2>
				<p className="text-sm text-muted-foreground">
					Define randomization unit identifiers and optional stratification
					factors.
				</p>
			</div>

			<div className="flex flex-col gap-2">
				<div className="flex items-center justify-between">
					<span className="text-sm font-medium text-foreground">
						Randomization Keys
						<span className="ml-0.5 text-destructive" aria-hidden="true">
							*
						</span>
					</span>
					<span className="text-xs text-muted-foreground">
						Identifies the unit being randomized
					</span>
				</div>
				<KeyEditor
					keys={data.keys}
					errors={errors}
					onAdd={onAddKey}
					onRemove={onRemoveKey}
					onUpdate={onUpdateKey}
				/>
			</div>

			<Separator />

			<div className="flex flex-col gap-2">
				<div>
					<span className="text-sm font-medium text-foreground">
						Stratification Factors
					</span>
					<p className="text-xs text-muted-foreground">
						Optional. Balance allocation across patient subgroups.
					</p>
				</div>
				<StratumEditor
					strata={data.strata}
					errors={errors}
					onAdd={onAddStratum}
					onAddPreset={onAddStratumPreset}
					onRemove={onRemoveStratum}
					onUpdate={onUpdateStratum}
				/>
			</div>
		</div>
	);
}
