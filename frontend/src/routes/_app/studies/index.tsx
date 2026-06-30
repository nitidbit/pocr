import { createFileRoute, Link } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import type { StudyResponse } from "#/api/client";
import { useStudies } from "#/api/queries";
import { formatDate } from "#/lib/format";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/studies/")({
	component: StudiesDashboardPage,
});

const STATUS_LABELS: Record<string, string> = {
	draft: "Draft",
	active: "Active",
	inactive: "Inactive",
};

const STATUS_COLORS: Record<string, string> = {
	draft: "bg-gray-100 text-gray-700",
	active: "bg-emerald-100 text-emerald-700",
	inactive: "bg-amber-100 text-amber-700",
};

function StudiesDashboardPage() {
	const { data: studies, isLoading } = useStudies();

	if (isLoading) {
		return (
			<div className="flex items-center justify-center py-16">
				<p className="text-sm text-muted-foreground">Loading studies...</p>
			</div>
		);
	}

	const list = (studies ?? []) as StudyResponse[];
	const activeCount = list.filter((s) => s.status === "active").length;
	const draftCount = list.filter((s) => s.status === "draft").length;
	const inactiveCount = list.filter((s) => s.status === "inactive").length;

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
				<Button render={<Link to="/studies/new" search={{ step: 1 }} />}>
					<Plus className="size-4" />
					Create New Study
				</Button>
			</div>

			<StatsRow
				activeCount={activeCount}
				totalStudies={list.length}
				draftCount={draftCount}
				inactiveCount={inactiveCount}
			/>

			{list.length === 0 ? <EmptyState /> : <StudiesTable studies={list} />}
		</div>
	);
}

interface StatsRowProps {
	activeCount: number;
	totalStudies: number;
	draftCount: number;
	inactiveCount: number;
}

function StatsRow({
	activeCount,
	totalStudies,
	draftCount,
	inactiveCount,
}: StatsRowProps) {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
			<StatCard label="Active Studies" value={activeCount} />
			<StatCard label="Total Studies" value={totalStudies} />
			<StatCard label="In Draft" value={draftCount} />
			<StatCard label="Inactive" value={inactiveCount} />
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
				<p className="mt-0.5 text-xs text-muted-foreground">
					{study.id.slice(0, 8)}...
				</p>
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

