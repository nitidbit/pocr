import { createFileRoute, Link } from "@tanstack/react-router";
import { AlertCircle, Plus, RefreshCw } from "lucide-react";
import type { StudyResponse } from "#/api/client";
import { useStudies } from "#/api/queries";
import { formatDate } from "#/lib/format";
import { STATUS_COLORS, STATUS_LABELS } from "#/lib/types/study";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/studies/")({
	component: StudiesDashboardPage,
});

function StudiesDashboardPage() {
	const { data: studies, isLoading, isError, refetch } = useStudies();

	if (isLoading) {
		return (
			<div className="flex flex-col gap-6">
				<div className="flex items-center justify-between">
					<div>
						<Skeleton className="h-8 w-32" />
						<Skeleton className="mt-2 h-4 w-64" />
					</div>
					<Skeleton className="h-9 w-40" />
				</div>
				<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
					{["active", "total", "draft", "closed"].map((stat) => (
						<Skeleton key={stat} className="h-20 rounded-lg" />
					))}
				</div>
				<Skeleton className="h-64 rounded-lg" />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<div className="flex flex-col items-center gap-2 text-center">
					<AlertCircle className="size-8 text-destructive" />
					<h2 className="text-lg font-semibold text-foreground">
						Failed to load studies
					</h2>
					<p className="text-sm text-muted-foreground">
						Something went wrong while fetching your studies.
					</p>
					<Button variant="outline" className="mt-2" onClick={() => refetch()}>
						<RefreshCw className="size-4" />
						Try again
					</Button>
				</div>
			</div>
		);
	}

	const list = studies ?? [];
	const activeCount = list.filter((s) => s.status === "active").length;
	const draftCount = list.filter((s) => s.status === "draft").length;
	const closedCount = list.filter(
		(s) => s.status === "closed" || s.status === "paused",
	).length;

	return (
		<div className="flex flex-col gap-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold tracking-tight text-foreground">
						Studies
					</h1>
					<p className="text-sm text-muted-foreground">
						Manage randomization studies and monitor enrollment progress.
					</p>
				</div>
				<Link
					to="/studies/new"
					search={{ step: 1 }}
					className={buttonVariants()}
				>
					<Plus className="size-4" />
					Create New Study
				</Link>
			</div>

			<StatsRow
				activeCount={activeCount}
				totalStudies={list.length}
				draftCount={draftCount}
				closedCount={closedCount}
			/>

			{list.length === 0 ? <EmptyState /> : <StudiesTable studies={list} />}
		</div>
	);
}

interface StatsRowProps {
	activeCount: number;
	totalStudies: number;
	draftCount: number;
	closedCount: number;
}

function StatsRow({
	activeCount,
	totalStudies,
	draftCount,
	closedCount,
}: StatsRowProps) {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
			<StatCard label="Active Studies" value={activeCount} />
			<StatCard label="Total Studies" value={totalStudies} />
			<StatCard label="In Draft" value={draftCount} />
			<StatCard label="Paused / Closed" value={closedCount} />
		</div>
	);
}

interface StatCardProps {
	label: string;
	value: number;
}

function StatCard({ label, value }: StatCardProps) {
	return (
		<div className="rounded-lg border bg-card p-5">
			<p className="text-sm font-medium text-muted-foreground">{label}</p>
			<p className="mt-1 text-3xl font-semibold tabular-nums text-foreground">
				{value}
			</p>
		</div>
	);
}

function EmptyState() {
	return (
		<div className="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16">
			<h2 className="text-lg font-medium text-foreground">No studies yet</h2>
			<p className="text-sm text-muted-foreground">
				Create your first randomization study to get started.
			</p>
			<Button render={<Link to="/studies/new" search={{ step: 1 }} />}>
				<Plus className="size-4" />
				Create New Study
			</Button>
		</div>
	);
}

function StudiesTable({ studies }: { studies: StudyResponse[] }) {
	return (
		<div className="overflow-hidden rounded-lg border">
			<table className="w-full text-sm">
				<thead className="border-b bg-muted/50">
					<tr>
						<th className="px-4 py-3 text-left font-medium text-muted-foreground">
							Name
						</th>
						<th className="px-4 py-3 text-left font-medium text-muted-foreground">
							Strategy
						</th>
						<th className="px-4 py-3 text-left font-medium text-muted-foreground">
							Status
						</th>
						<th className="px-4 py-3 text-right font-medium text-muted-foreground">
							Arms
						</th>
						<th className="px-4 py-3 text-right font-medium text-muted-foreground">
							Last Updated
						</th>
					</tr>
				</thead>
				<tbody>
					{studies.map((study) => (
						<StudyRow key={study.id} study={study} />
					))}
				</tbody>
			</table>
		</div>
	);
}

function StudyRow({ study }: { study: StudyResponse }) {
	return (
		<tr className="border-b last:border-b-0 hover:bg-muted/30">
			<td className="px-4 py-3">
				<Link
					to="/studies/$studyId"
					params={{ studyId: study.id }}
					className="font-medium text-foreground hover:text-primary hover:underline"
				>
					{study.name}
				</Link>
				<p className="mt-0.5 text-xs text-muted-foreground">{study.id}</p>
			</td>
			<td className="px-4 py-3 text-muted-foreground">
				{study.randomizationConfig.strategy}
			</td>
			<td className="px-4 py-3">
				<Badge
					className={STATUS_COLORS[study.status] ?? "bg-gray-100 text-gray-700"}
				>
					{STATUS_LABELS[study.status] ?? study.status}
				</Badge>
			</td>
			<td className="px-4 py-3 text-right tabular-nums text-foreground">
				{study.randomizationConfig.arms.length}
			</td>
			<td className="px-4 py-3 text-right text-muted-foreground">
				{formatDate(study.updatedAt)}
			</td>
		</tr>
	);
}
