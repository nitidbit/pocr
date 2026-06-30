import { createFileRoute } from "@tanstack/react-router";
import { Calendar } from "lucide-react";
import type { StudyResponse } from "#/api/client";
import { useStudy } from "#/api/queries";
import { formatDate } from "#/lib/format";

export const Route = createFileRoute("/_app/studies/$studyId/")({
	component: StudyOverviewPage,
});

function StudyOverviewPage() {
	const { studyId } = Route.useParams();
	const { data: study } = useStudy(studyId);

	if (!study) return null;

	return (
		<div className="grid gap-6 lg:grid-cols-3">
			<div className="flex flex-col gap-6 lg:col-span-2">
				<DescriptionSection description={study.description} />
				<ArmsOverviewSection study={study} />
			</div>

			<div className="flex flex-col gap-6">
				<MetadataCard study={study} />
			</div>
		</div>
	);
}

function DescriptionSection({ description }: { description: string }) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-2 text-sm font-medium text-muted-foreground">
				Description
			</h2>
			<p className="text-sm leading-relaxed text-foreground">
				{description || "No description provided."}
			</p>
		</section>
	);
}

function ArmsOverviewSection({ study }: { study: StudyResponse }) {
	const arms = study.randomizationConfig.arms;
	const totalRatio = arms.reduce((sum, arm) => sum + arm.ratio, 0);

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-3 text-sm font-medium text-muted-foreground">
				Treatment Arms
			</h2>
			<div className="overflow-hidden rounded-md border">
				<table className="w-full text-sm">
					<thead className="border-b bg-muted/50">
						<tr>
							<th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
								Arm ID
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
								<td className="px-4 py-2.5 font-mono text-xs text-foreground">
									{arm.id}
								</td>
								<td className="px-4 py-2.5 text-right tabular-nums text-foreground">
									{arm.ratio}
								</td>
								<td className="px-4 py-2.5 text-right tabular-nums text-muted-foreground">
									{totalRatio > 0
										? Math.round((arm.ratio / totalRatio) * 100)
										: 0}
									%
								</td>
								<td className="px-4 py-2.5 text-right tabular-nums text-muted-foreground">
									{arm.locator ?? "—"}
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</section>
	);
}

function MetadataCard({ study }: { study: StudyResponse }) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-3 text-sm font-medium text-muted-foreground">
				Details
			</h2>
			<dl className="flex flex-col gap-3 text-sm">
				<MetadataRow label="Status" value={study.status} />
				<MetadataRow
					label="Strategy"
					value={study.randomizationConfig.strategy}
				/>
				<MetadataRow
					label="Arms"
					value={String(study.randomizationConfig.arms.length)}
				/>
				{study.randomizationConfig.blockSize && (
					<MetadataRow
						label="Block Size"
						value={String(study.randomizationConfig.blockSize)}
					/>
				)}
				<MetadataRow
					label="Created"
					value={formatDate(study.createdAt)}
					icon={<Calendar className="size-3.5 text-muted-foreground" />}
				/>
				<MetadataRow
					label="Updated"
					value={formatDate(study.updatedAt)}
					icon={<Calendar className="size-3.5 text-muted-foreground" />}
				/>
			</dl>
		</section>
	);
}

interface MetadataRowProps {
	label: string;
	value: string;
	icon?: React.ReactNode;
}

function MetadataRow({ label, value, icon }: MetadataRowProps) {
	return (
		<div className="flex items-center justify-between">
			<dt className="text-muted-foreground">{label}</dt>
			<dd className="flex items-center gap-1.5 font-medium text-foreground">
				{icon}
				{value}
			</dd>
		</div>
	);
}

