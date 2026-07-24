import { createFileRoute } from "@tanstack/react-router";
import { Activity, BarChart3, Scale, Trash2, TrendingUp } from "lucide-react";
import { useState } from "react";
import type {
	BalanceResponse,
	BalanceVariable,
	RandomizationResponse,
} from "#/api/client";
import {
	useBalance,
	usePurgeNonProduction,
	useRandomizations,
	useStudy,
} from "#/api/queries";
import { ENVIRONMENTS, useEnvironment } from "#/lib/environment";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/studies/$studyId/monitor")({
	component: StudyMonitorPage,
});

function StudyMonitorPage() {
	const { studyId } = Route.useParams();
	const { environment } = useEnvironment();
	const { data: study } = useStudy(studyId);
	const { data: randomizations, isLoading } = useRandomizations(
		studyId,
		environment,
	);
	const { data: balance, isLoading: balanceLoading } = useBalance(
		studyId,
		environment,
	);

	return (
		<div className="flex flex-col gap-6">
			<p className="text-xs text-muted-foreground">
				Showing {ENVIRONMENTS[environment].label} data
			</p>
			<BalanceSummary study={study} randomizations={randomizations} />
			{environment === "nonproduction" ? (
				<DemographicBalanceUnavailable />
			) : (
				<DemographicBalance balance={balance} isLoading={balanceLoading} />
			)}
			<RecentRandomizations
				randomizations={randomizations}
				isLoading={isLoading}
			/>
			{environment === "nonproduction" && <PurgeTestData studyId={studyId} />}
		</div>
	);
}

function PurgeTestData({ studyId }: { studyId: string }) {
	const purge = usePurgeNonProduction(studyId);
	const [showConfirm, setShowConfirm] = useState(false);

	return (
		<section className="rounded-lg border border-destructive/30 bg-destructive/5 p-5">
			<h2 className="mb-2 text-sm font-semibold text-destructive">
				Reset Test Data
			</h2>
			{!showConfirm ? (
				<div className="flex items-center justify-between">
					<p className="text-sm text-muted-foreground">
						Purge all non-production randomizations for this study.
					</p>
					<Button
						variant="destructive"
						size="sm"
						onClick={() => setShowConfirm(true)}
					>
						<Trash2 className="size-4" />
						Purge Non-Production Data
					</Button>
				</div>
			) : (
				<div className="flex flex-col gap-3">
					<p className="text-sm text-foreground">
						This permanently deletes all non-production randomizations and
						cached allocation blocks for this study. This action cannot be
						undone.
					</p>
					<div className="flex items-center gap-2">
						<Button
							variant="destructive"
							size="sm"
							onClick={() =>
								purge.mutate(undefined, {
									onSuccess: () => setShowConfirm(false),
								})
							}
							disabled={purge.isPending}
						>
							{purge.isPending ? "Purging..." : "Confirm Purge"}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setShowConfirm(false)}
							disabled={purge.isPending}
						>
							Cancel
						</Button>
					</div>
				</div>
			)}
		</section>
	);
}

function BalanceSummary({
	study,
	randomizations,
}: {
	study:
		| { randomizationConfig: { arms: Array<{ id: string; ratio: number }> } }
		| undefined;
	randomizations: RandomizationResponse[] | undefined;
}) {
	if (!study || !randomizations) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<Scale className="size-4" />
					Balance Summary
				</h2>
				<p className="text-sm text-muted-foreground">Loading...</p>
			</section>
		);
	}

	const armCounts = study.randomizationConfig.arms.map((arm) => ({
		id: arm.id,
		ratio: arm.ratio,
		count: randomizations.filter((r) => r.arm.id === arm.id).length,
	}));
	const total = randomizations.length;

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<Scale className="size-4" />
				Balance Summary
			</h2>
			{total === 0 ? (
				<p className="text-sm text-muted-foreground">No randomizations yet.</p>
			) : (
				<div className="flex flex-col gap-3">
					{armCounts.map((arm) => {
						const percent =
							total > 0 ? Math.round((arm.count / total) * 100) : 0;
						return (
							<div key={arm.id} className="flex flex-col gap-1">
								<div className="flex items-center justify-between text-sm">
									<span className="font-mono text-xs text-foreground">
										{arm.id.slice(0, 8)}...
									</span>
									<span className="tabular-nums text-muted-foreground">
										{arm.count} ({percent}%)
									</span>
								</div>
								<div className="h-2 overflow-hidden rounded-full bg-secondary">
									<div
										className="h-full rounded-full bg-primary transition-all"
										style={{ width: `${percent}%` }}
									/>
								</div>
							</div>
						);
					})}
					<p className="mt-2 text-xs text-muted-foreground">
						Total: {total} randomizations
					</p>
				</div>
			)}
		</section>
	);
}

function DemographicBalance({
	balance,
	isLoading,
}: {
	balance: BalanceResponse | undefined;
	isLoading: boolean;
}) {
	if (isLoading) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<TrendingUp className="size-4" />
					Demographic Balance
				</h2>
				<div className="flex flex-col gap-2">
					{[1, 2, 3].map((i) => (
						<div key={i} className="h-8 animate-pulse rounded bg-muted/50" />
					))}
				</div>
			</section>
		);
	}

	if (!balance || balance.total_patients === 0) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<TrendingUp className="size-4" />
					Demographic Balance
				</h2>
				<div className="flex h-32 items-center justify-center rounded-md border border-dashed bg-muted/30">
					<p className="text-sm text-muted-foreground">
						No demographics synced yet.
					</p>
				</div>
			</section>
		);
	}

	const armIds = balance.arms.map((a) => a.id);

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<TrendingUp className="size-4" />
				Demographic Balance
				<span className="ml-auto text-xs tabular-nums">
					{balance.total_patients} patients
				</span>
			</h2>
			<div className="overflow-x-auto rounded-md border">
				<table className="w-full text-sm">
					<thead className="border-b bg-muted/50">
						<tr>
							<th className="px-4 py-2 text-left font-medium text-muted-foreground">
								Variable
							</th>
							{armIds.map((id) => (
								<th
									key={id}
									className="px-4 py-2 text-right font-medium text-muted-foreground"
								>
									{id.slice(0, 8)}...
									<span className="ml-1 text-xs">
										(n=
										{balance.arms.find((a) => a.id === id)?.count})
									</span>
								</th>
							))}
							<th className="px-4 py-2 text-right font-medium text-muted-foreground">
								SMD
							</th>
						</tr>
					</thead>
					<tbody>
						{balance.variables.map((variable) => (
							<VariableRows
								key={variable.name}
								variable={variable}
								armIds={armIds}
							/>
						))}
					</tbody>
				</table>
			</div>
		</section>
	);
}

// The backend /balance endpoint does not yet partition by environment, so it
// would return production demographics even in the non-production view. Rather
// than mislabel production data as test data, show a placeholder until the
// backend honors the environment param.
function DemographicBalanceUnavailable() {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<TrendingUp className="size-4" />
				Demographic Balance
			</h2>
			<div className="flex h-32 items-center justify-center rounded-md border border-dashed bg-muted/30">
				<p className="max-w-md text-center text-sm text-muted-foreground">
					Demographic balance is not yet partitioned by environment.
				</p>
			</div>
		</section>
	);
}

function SmdBadge({ value }: { value: number }) {
	const formatted = value.toFixed(2);
	if (value > 0.25) {
		return (
			<span className="inline-block rounded px-1.5 py-0.5 text-xs font-medium tabular-nums bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-400">
				{formatted}
			</span>
		);
	}
	if (value > 0.1) {
		return (
			<span className="inline-block rounded px-1.5 py-0.5 text-xs font-medium tabular-nums bg-yellow-100 text-yellow-700 dark:bg-yellow-950 dark:text-yellow-400">
				{formatted}
			</span>
		);
	}
	return (
		<span className="inline-block rounded px-1.5 py-0.5 text-xs font-medium tabular-nums bg-green-100 text-green-700 dark:bg-green-950 dark:text-green-400">
			{formatted}
		</span>
	);
}

function VariableRows({
	variable,
	armIds,
}: {
	variable: BalanceVariable;
	armIds: string[];
}) {
	if (variable.type === "continuous") {
		return (
			<tr className="border-b last:border-b-0">
				<td className="px-4 py-2 font-medium text-foreground capitalize">
					{variable.name}
				</td>
				{armIds.map((armId) => {
					const arm = variable.arms.find((a) => a.arm_id === armId);
					if (!arm) return <td key={armId} className="px-4 py-2" />;
					return (
						<td
							key={armId}
							className="px-4 py-2 text-right tabular-nums text-foreground"
						>
							{arm.mean.toFixed(1)}{" "}
							<span className="text-muted-foreground">±</span>{" "}
							{arm.sd.toFixed(1)}
							{arm.missing > 0 && (
								<span className="ml-1 text-xs text-muted-foreground">
									({arm.missing} missing)
								</span>
							)}
						</td>
					);
				})}
				<td className="px-4 py-2 text-right">
					<SmdBadge value={variable.smd} />
				</td>
			</tr>
		);
	}

	return (
		<>
			{variable.categories.map((category, idx) => (
				<tr key={category.value} className="border-b last:border-b-0">
					<td className="px-4 py-2 text-foreground">
						{idx === 0 && (
							<span className="font-medium capitalize">{variable.name}</span>
						)}
						<span className={idx === 0 ? "ml-2" : "ml-6"}>
							{category.value}
						</span>
					</td>
					{armIds.map((armId) => {
						const arm = category.arms.find((a) => a.arm_id === armId);
						if (!arm) return <td key={armId} className="px-4 py-2" />;
						const countDisplay =
							typeof arm.count === "string" ? arm.count : String(arm.count);
						const pctDisplay =
							typeof arm.pct === "string" ? arm.pct : `${arm.pct}%`;
						return (
							<td
								key={armId}
								className="px-4 py-2 text-right tabular-nums text-foreground"
							>
								{countDisplay}{" "}
								<span className="text-muted-foreground">({pctDisplay})</span>
							</td>
						);
					})}
					<td className="px-4 py-2 text-right">
						{idx === 0 && <SmdBadge value={variable.smd} />}
					</td>
				</tr>
			))}
		</>
	);
}

function RecentRandomizations({
	randomizations,
	isLoading,
}: {
	randomizations: RandomizationResponse[] | undefined;
	isLoading: boolean;
}) {
	if (isLoading) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<BarChart3 className="size-4" />
					Recent Randomizations
				</h2>
				<p className="text-sm text-muted-foreground">Loading...</p>
			</section>
		);
	}

	if (!randomizations || randomizations.length === 0) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<BarChart3 className="size-4" />
					Recent Randomizations
				</h2>
				<div className="flex h-32 items-center justify-center rounded-md border border-dashed bg-muted/30">
					<p className="text-sm text-muted-foreground">
						No randomizations recorded yet.
					</p>
				</div>
			</section>
		);
	}

	const recent = randomizations.slice(0, 10);

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<Activity className="size-4" />
				Recent Randomizations ({randomizations.length} total)
			</h2>
			<div className="overflow-hidden rounded-md border">
				<table className="w-full text-sm">
					<thead className="border-b bg-muted/50">
						<tr>
							<th className="px-4 py-2 text-left font-medium text-muted-foreground">
								ID
							</th>
							<th className="px-4 py-2 text-left font-medium text-muted-foreground">
								Arm
							</th>
							<th className="px-4 py-2 text-right font-medium text-muted-foreground">
								Issued At
							</th>
						</tr>
					</thead>
					<tbody>
						{recent.map((r) => (
							<tr key={r.id} className="border-b last:border-b-0">
								<td className="px-4 py-2 font-mono text-xs text-foreground">
									{r.id.slice(0, 8)}...
								</td>
								<td className="px-4 py-2 font-mono text-xs text-foreground">
									{r.arm.id.slice(0, 8)}...
								</td>
								<td className="px-4 py-2 text-right text-muted-foreground">
									{new Date(r.issuedAt).toLocaleString()}
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</section>
	);
}
