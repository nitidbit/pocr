import { createFileRoute } from "@tanstack/react-router";
import { AlertCircle, Check, RefreshCw } from "lucide-react";
import { useState } from "react";
import type { StudyResponse } from "#/api/client";
import { useSetArmLocator, useStudy } from "#/api/queries";
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
	const totalRatio = arms.reduce((sum, arm) => sum + arm.ratio, 0);
	const setLocator = useSetArmLocator(studyId);

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2
				id="arms-heading"
				className="mb-1 text-sm font-medium text-muted-foreground"
			>
				Treatment Arms
			</h2>
			<p className="mb-3 text-xs text-muted-foreground">
				Map each arm to its Epic criteria ID (locator) so CDS Hooks requests
				resolve to this study.
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
						{arms.map((arm) => (
							<tr key={arm.id} className="border-b last:border-b-0">
								<td className="px-4 py-2.5 text-foreground">{arm.name}</td>
								<td className="px-4 py-2.5 text-right tabular-nums text-foreground">
									{arm.ratio}
								</td>
								<td className="px-4 py-2.5 text-right tabular-nums text-muted-foreground">
									{totalRatio > 0
										? Math.round((arm.ratio / totalRatio) * 100)
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
						))}
					</tbody>
				</table>
			</div>
		</section>
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
