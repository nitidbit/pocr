import { createFileRoute } from "@tanstack/react-router";
import { Activity, BarChart3, Scale, Trash2 } from "lucide-react";
import { useState } from "react";
import type { RandomizationCounts, RandomizationResponse } from "#/api/client";
import {
	usePurgeNonProduction,
	useRandomizationCounts,
	useRandomizations,
	useStudy,
} from "#/api/queries";
import { DatasetReports } from "#/components/studies/dataset-reports";
import { ENVIRONMENTS, useEnvironment } from "#/lib/environment";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/studies/$studyId/monitor")({
	component: StudyMonitorPage,
});

// Prefer the human-readable arm label; fall back to the full arm id when the
// name is empty or absent so a row never renders a blank arm.
export function armDisplayName(arm: { id: string; name?: string }): string {
	return arm.name?.trim() ? arm.name : arm.id;
}

function StudyMonitorPage() {
	const { studyId } = Route.useParams();
	const { environment } = useEnvironment();
	const { data: study } = useStudy(studyId);
	const { data: randomizations, isLoading } = useRandomizations(
		studyId,
		environment,
	);
	const { data: counts } = useRandomizationCounts(studyId, environment);

	return (
		<div className="flex flex-col gap-6">
			<p className="text-xs text-muted-foreground">
				Showing {ENVIRONMENTS[environment].label} data
			</p>
			<BalanceSummary study={study} counts={counts} />
			<DatasetReports studyId={studyId} environment={environment} />
			<RecentRandomizations
				randomizations={randomizations}
				total={counts?.total}
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
	counts,
}: {
	study:
		| {
				randomizationConfig: {
					arms: Array<{ id: string; name: string; ratio: number }>;
				};
		  }
		| undefined;
	counts: RandomizationCounts | undefined;
}) {
	if (!study || !counts) {
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

	// Counts are aggregated server-side over the whole study, so the summary is
	// accurate regardless of how many records the list endpoint returns.
	const countByArm = new Map(counts.arms.map((a) => [a.armId, a.count]));
	const armCounts = study.randomizationConfig.arms.map((arm) => ({
		id: arm.id,
		name: armDisplayName(arm),
		ratio: arm.ratio,
		count: countByArm.get(arm.id) ?? 0,
	}));
	const total = counts.total;

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
									<span className="text-foreground">{arm.name}</span>
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

function RecentRandomizations({
	randomizations,
	total,
	isLoading,
}: {
	randomizations: RandomizationResponse[] | undefined;
	// Whole-study total from the counts endpoint; the table itself only renders
	// the newest few rows, so its length is not the study total.
	total: number | undefined;
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
	const totalLabel = total ?? randomizations.length;

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<Activity className="size-4" />
				Recent Randomizations ({totalLabel} total)
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
									{r.id}
								</td>
								<td className="px-4 py-2 text-xs text-foreground">
									{armDisplayName(r.arm)}
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
