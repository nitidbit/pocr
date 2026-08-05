import { RandomizationStrategy } from "#/lib/types/randomization";
import { Input } from "@/components/ui/input";

import { ArmEditor } from "./arm-editor";
import { FormField } from "./form-field";
import type { StepProps } from "./types";

interface StepRandomizationDesignProps extends StepProps {
	onAddArm: () => void;
	onRemoveArm: (id: string) => void;
	onUpdateArm: (id: string, field: string, value: string) => void;
}

const STRATEGY_OPTIONS = [
	{ value: RandomizationStrategy.Simple, label: "Simple Randomization" },
	{ value: RandomizationStrategy.Block, label: "Block Randomization" },
];

/** Step 3: Randomization Design — strategy, block size, treatment arms. */
export function StepRandomizationDesign({
	data,
	errors,
	onChange,
	onAddArm,
	onRemoveArm,
	onUpdateArm,
}: StepRandomizationDesignProps) {
	const showBlockSize = data.strategy === RandomizationStrategy.Block;

	return (
		<div className="flex flex-col gap-6">
			<div>
				<h2 className="text-lg font-semibold text-foreground">
					Randomization Design
				</h2>
				<p className="text-sm text-muted-foreground">
					Configure the randomization strategy and treatment arms. Block
					randomization becomes stratified automatically when you add
					stratification factors.
				</p>
			</div>

			<div className="grid gap-4 sm:grid-cols-2">
				<FormField
					label="Strategy"
					name="strategy"
					required
					error={errors.strategy}
				>
					<select
						className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm"
						value={data.strategy}
						onChange={(e) => onChange("strategy", e.target.value)}
					>
						<option value="">Select a strategy...</option>
						{STRATEGY_OPTIONS.map((opt) => (
							<option key={opt.value} value={opt.value}>
								{opt.label}
							</option>
						))}
					</select>
				</FormField>

				{showBlockSize && (
					<FormField
						label="Block Size"
						name="blockSize"
						error={errors.blockSize}
					>
						<Input
							type="number"
							min={2}
							placeholder="e.g. 4"
							value={data.blockSize}
							onChange={(e) => onChange("blockSize", e.target.value)}
						/>
					</FormField>
				)}
			</div>

			<div className="flex flex-col gap-2">
				<div className="flex items-center justify-between">
					<span className="text-sm font-medium text-foreground">
						Treatment Arms
						<span className="ml-0.5 text-destructive" aria-hidden="true">
							*
						</span>
					</span>
					<span className="text-xs text-muted-foreground">
						Name + allocation ratio
					</span>
				</div>
				<ArmEditor
					arms={data.arms}
					errors={errors}
					onAdd={onAddArm}
					onRemove={onRemoveArm}
					onUpdate={onUpdateArm}
				/>
			</div>
		</div>
	);
}
