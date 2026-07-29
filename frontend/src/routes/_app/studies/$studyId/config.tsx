import { createFileRoute } from "@tanstack/react-router";
import { AlertCircle, Check, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { StudyResponse } from "#/api/client";
import { useSetArmLocator, useStudy, useUpdateRatios } from "#/api/queries";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/studies/$studyId/config")({
	component: StudyConfigPage,
});

function StudyConfigPage() {
	const { studyId } = Route.useParams();
	const { data: study, isLoading, isError, refetch } = useStudy(studyId);

	if (isLoading) {
		return (
			<div className="flex flex-col gap-6">
				<Skeleton className="h-20 rounded-lg" />
				<Skeleton className="h-48 rounded-lg" />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<AlertCircle className="size-8 text-destructive" />
				<h2 className="text-lg font-semibold">Failed to load configuration</h2>
				<Button variant="outline" onClick={() => refetch()}>
					<RefreshCw className="size-4" />
					Try again
				</Button>
			</div>
		);
	}

	if (!study) return null;

	const config = study.randomizationConfig;

	return (
		<div className="flex flex-col gap-6">
			<StrategySection config={config} />
			<ArmsSection studyId={studyId} arms={config.arms} />
			{config.strata.length > 0 && <StrataSection strata={config.strata} />}
			{config.blockSize && <BlockSizeSection blockSize={config.blockSize} />}
		</div>
	);
}

function StrategySection({
	config,
}: {
	config: StudyResponse["randomizationConfig"];
}) {
	const labels: Record<string, string> = {
		simple: "Simple Randomization",
		block: "Block Randomization",
	};

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-1 text-sm font-medium text-muted-foreground">
				Randomization Strategy
			</h2>
			<p className="text-lg font-semibold text-foreground">
				{labels[config.strategy] ?? config.strategy}
			</p>
		</section>
	);
}

function ArmsSection({
	studyId,
	arms,
}: {
	studyId: string;
	arms: StudyResponse["randomizationConfig"]["arms"];
}) {
	const setLocator = useSetArmLocator(studyId);
	const updateRatios = useUpdateRatios(studyId);

	// Staged ratio edits keyed by arm id; absent means "unchanged".
	const [draft, setDraft] = useState<Record<string, string>>({});
	const [showConfirm, setShowConfirm] = useState(false);

	const ratioFor = (arm: (typeof arms)[number]) => {
		const raw = draft[arm.id];
		if (raw === undefined) return { value: arm.ratio, isValid: true };
		const parsed = Number(raw);
		const isValid =
			raw.trim() !== "" && Number.isInteger(parsed) && parsed >= 0;
		return { value: isValid ? parsed : arm.ratio, isValid };
	};

	const changes = arms
		.filter((arm) => draft[arm.id] !== undefined)
		.map((arm) => ({ arm, ...ratioFor(arm) }))
		.filter((entry) => entry.isValid && entry.value !== entry.arm.ratio);

	const hasInvalidEntry = arms.some((arm) => !ratioFor(arm).isValid);
	const previewTotal = arms.reduce((sum, arm) => sum + ratioFor(arm).value, 0);
	const isDirty = changes.length > 0;
	const canSave = isDirty && !hasInvalidEntry && previewTotal > 0;

	// Collapse the confirm panel whenever edits revert to unchanged, so a fresh
	// edit always re-enters through "Review changes".
	useEffect(() => {
		if (!isDirty) setShowConfirm(false);
	}, [isDirty]);

	const resetDraft = () => {
		setDraft({});
		setShowConfirm(false);
	};

	const handleConfirm = () => {
		updateRatios.mutate(
			changes.map(({ arm, value }) => ({ armId: arm.id, ratio: value })),
			{ onSuccess: resetDraft },
		);
	};

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2
				id="arms-heading"
				className="mb-1 text-sm font-medium text-muted-foreground"
			>
				Treatment Arms
			</h2>
			<p className="mb-3 text-xs text-muted-foreground">
				Edit each arm's allocation ratio, and map it to its Epic criteria ID
				(locator) so CDS Hooks requests resolve to this study.
			</p>
			<div className="overflow-hidden rounded-md border">
				<table className="w-full text-sm" aria-labelledby="arms-heading">
					<thead className="border-b bg-muted/50">
						<tr>
							<th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
								Name
							</th>
							<th className="px-4 py-2.5 text-right font-medium text-muted-foreground">
								Ratio
							</th>
							<th className="px-4 py-2.5 text-right font-medium text-muted-foreground">
								Allocation %
							</th>
							<th className="px-4 py-2.5 text-right font-medium text-muted-foreground">
								Locator
							</th>
						</tr>
					</thead>
					<tbody>
						{arms.map((arm) => {
							const { value, isValid } = ratioFor(arm);
							return (
								<tr key={arm.id} className="border-b last:border-b-0">
									<td className="px-4 py-2.5 text-foreground">{arm.name}</td>
									<td className="px-4 py-2.5">
										<RatioCell
											armName={arm.name}
											value={draft[arm.id] ?? String(arm.ratio)}
											isValid={isValid}
											disabled={updateRatios.isPending}
											onChange={(next) =>
												setDraft((prev) => ({ ...prev, [arm.id]: next }))
											}
										/>
									</td>
									<td className="px-4 py-2.5 text-right tabular-nums text-muted-foreground">
										{previewTotal > 0
											? Math.round((value / previewTotal) * 100)
											: 0}
										%
									</td>
									<td className="px-4 py-2.5">
										<LocatorCell
											armId={arm.id}
											currentLocator={arm.locator}
											onSave={(locator) =>
												setLocator.mutate({ armId: arm.id, locator })
											}
											isPending={setLocator.isPending}
										/>
									</td>
								</tr>
							);
						})}
					</tbody>
				</table>
			</div>

			{isDirty &&
				(showConfirm ? (
					<div className="mt-3 flex flex-col gap-3 rounded-md border border-primary/30 bg-primary/5 p-3">
						<p className="text-sm text-foreground">
							Apply the new allocation ratios? The change takes effect for{" "}
							<strong>future allocations only</strong> — subjects already
							randomized, and any in-flight randomization block, keep their
							current assignment. This change is recorded in the study's audit
							history.
						</p>
						<div className="flex items-center gap-2">
							<Button
								size="sm"
								onClick={handleConfirm}
								disabled={!canSave || updateRatios.isPending}
							>
								{updateRatios.isPending ? "Applying..." : "Apply ratios"}
							</Button>
							<Button
								variant="outline"
								size="sm"
								onClick={() => setShowConfirm(false)}
								disabled={updateRatios.isPending}
							>
								Back
							</Button>
						</div>
					</div>
				) : (
					<div className="mt-3 flex items-center justify-end gap-2">
						<Button variant="outline" size="sm" onClick={resetDraft}>
							Discard
						</Button>
						<Button
							size="sm"
							onClick={() => setShowConfirm(true)}
							disabled={!canSave}
						>
							Review changes
						</Button>
					</div>
				))}
		</section>
	);
}

function RatioCell({
	armName,
	value,
	isValid,
	disabled,
	onChange,
}: {
	armName: string;
	value: string;
	isValid: boolean;
	disabled: boolean;
	onChange: (value: string) => void;
}) {
	return (
		<div className="flex items-center justify-end">
			<Input
				type="number"
				inputMode="numeric"
				min={0}
				step={1}
				aria-label={`Ratio for arm ${armName}`}
				aria-invalid={!isValid}
				value={value}
				disabled={disabled}
				onChange={(e) => onChange(e.target.value)}
				className="h-8 w-20 text-right tabular-nums"
			/>
		</div>
	);
}

function LocatorCell({
	armId,
	currentLocator,
	onSave,
	isPending,
}: {
	armId: string;
	currentLocator?: number;
	onSave: (locator: number) => void;
	isPending: boolean;
}) {
	const [value, setValue] = useState(
		currentLocator != null ? String(currentLocator) : "",
	);

	const parsed = Number(value);
	const isValid =
		value.trim() !== "" && Number.isInteger(parsed) && parsed >= 0;
	const isDirty =
		value.trim() !== (currentLocator != null ? String(currentLocator) : "");

	return (
		<form
			className="flex items-center justify-end gap-2"
			onSubmit={(e) => {
				e.preventDefault();
				if (isValid && isDirty) onSave(parsed);
			}}
		>
			<Input
				type="number"
				inputMode="numeric"
				min={0}
				step={1}
				aria-label={`Locator for arm ${armId}`}
				placeholder="—"
				value={value}
				onChange={(e) => setValue(e.target.value)}
				className="h-8 w-28 text-right tabular-nums"
			/>
			<Button
				type="submit"
				size="icon"
				variant="outline"
				className="size-8 shrink-0"
				disabled={!isValid || !isDirty || isPending}
				aria-label="Save locator"
			>
				<Check className="size-4" />
			</Button>
		</form>
	);
}

function StrataSection({
	strata,
}: {
	strata: StudyResponse["randomizationConfig"]["strata"];
}) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-3 text-sm font-medium text-muted-foreground">
				Stratification Factors
			</h2>
			<div className="flex flex-col gap-3">
				{strata.map((stratum) => (
					<div key={stratum.id} className="rounded-md border p-3">
						<div className="flex items-center gap-2">
							<span className="text-sm font-medium text-foreground">
								{stratum.name}
							</span>
							<span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
								{stratum.context}
							</span>
						</div>
						<p className="mt-1 text-xs text-muted-foreground">
							Path: {stratum.path}
						</p>
					</div>
				))}
			</div>
		</section>
	);
}

function BlockSizeSection({ blockSize }: { blockSize: number }) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-1 text-sm font-medium text-muted-foreground">
				Block Size
			</h2>
			<p className="text-lg font-semibold tabular-nums text-foreground">
				{blockSize}
			</p>
		</section>
	);
}
