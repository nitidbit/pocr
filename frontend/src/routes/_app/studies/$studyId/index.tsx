import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { AlertCircle, Calendar, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";
import type { StudyResponse } from "#/api/client";
import { useDeleteStudy, useStudy, useUpdateStudy } from "#/api/queries";
import { formatDate } from "#/lib/format";
import { STATUS_COLORS, STATUS_LABELS } from "#/lib/types/study";
import type { StudyStatus } from "#/lib/types/study";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/studies/$studyId/")({
	component: StudyOverviewPage,
});

function StudyOverviewPage() {
	const { studyId } = Route.useParams();
	const { data: study, isLoading, isError, refetch } = useStudy(studyId);

	if (isLoading) {
		return (
			<div className="grid gap-6 lg:grid-cols-3">
				<div className="flex flex-col gap-6 lg:col-span-2">
					<Skeleton className="h-32 rounded-lg" />
					<Skeleton className="h-48 rounded-lg" />
				</div>
				<Skeleton className="h-64 rounded-lg" />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<AlertCircle className="size-8 text-destructive" />
				<h2 className="text-lg font-semibold">Failed to load study</h2>
				<Button variant="outline" onClick={() => refetch()}>
					<RefreshCw className="size-4" />
					Try again
				</Button>
			</div>
		);
	}

	if (!study) return null;

	return (
		<div className="flex flex-col gap-6">
			<StatusSection study={study} />

			<div className="grid gap-6 lg:grid-cols-3">
				<div className="flex flex-col gap-6 lg:col-span-2">
					<DescriptionSection description={study.description} />
					<ArmsOverviewSection study={study} />
				</div>

				<div className="flex flex-col gap-6">
					<MetadataCard study={study} />
				</div>
			</div>

			{study.status === "draft" && <DangerZone study={study} />}
		</div>
	);
}

function StatusSection({ study }: { study: StudyResponse }) {
	const updateStudy = useUpdateStudy(study.id);
	const [confirming, setConfirming] = useState<"activate" | "deactivate" | null>(null);

	const handleStatusChange = (status: StudyStatus) => {
		updateStudy.mutate({ status }, { onSuccess: () => setConfirming(null) });
	};

	return (
		<section className="flex items-center justify-between rounded-lg border bg-card p-4">
			<div className="flex items-center gap-3">
				<Badge className={STATUS_COLORS[study.status as StudyStatus] ?? "bg-gray-100 text-gray-700"}>
					{STATUS_LABELS[study.status as StudyStatus] ?? study.status}
				</Badge>
				{study.status === "active" && (
					<span className="text-xs text-muted-foreground">
						Accepting randomizations via CDS Hooks
					</span>
				)}
			</div>

			<div className="flex items-center gap-2">
				{study.status === "draft" && !confirming && (
					<Button
						variant="outline"
						size="sm"
						onClick={() => setConfirming("activate")}
					>
						Activate Study
					</Button>
				)}
				{study.status === "active" && !confirming && (
					<Button
						variant="outline"
						size="sm"
						onClick={() => setConfirming("deactivate")}
					>
						Deactivate Study
					</Button>
				)}

				{confirming === "activate" && (
					<div className="flex items-center gap-2">
						<span className="text-xs text-muted-foreground">
							Activating enables randomization via CDS Hooks.
						</span>
						<Button
							size="sm"
							onClick={() => handleStatusChange("active")}
							disabled={updateStudy.isPending}
						>
							Confirm
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setConfirming(null)}
						>
							Cancel
						</Button>
					</div>
				)}
				{confirming === "deactivate" && (
					<div className="flex items-center gap-2">
						<span className="text-xs text-muted-foreground">
							Deactivating stops new randomizations. Data is preserved.
						</span>
						<Button
							size="sm"
							onClick={() => handleStatusChange("inactive")}
							disabled={updateStudy.isPending}
						>
							Confirm
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setConfirming(null)}
						>
							Cancel
						</Button>
					</div>
				)}
			</div>
		</section>
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
								<td className="px-4 py-2.5 text-foreground">
									{arm.name}
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

function DangerZone({ study }: { study: StudyResponse }) {
	const deleteStudy = useDeleteStudy();
	const navigate = useNavigate();
	const [showConfirm, setShowConfirm] = useState(false);

	const handleDelete = () => {
		deleteStudy.mutate(study.id, {
			onSuccess: () => navigate({ to: "/studies" }),
		});
	};

	return (
		<section className="rounded-lg border border-destructive/30 bg-destructive/5 p-5">
			<h2 className="mb-2 text-sm font-semibold text-destructive">
				Danger Zone
			</h2>
			{!showConfirm ? (
				<div className="flex items-center justify-between">
					<p className="text-sm text-muted-foreground">
						Permanently delete this study and all its data.
					</p>
					<Button
						variant="destructive"
						size="sm"
						onClick={() => setShowConfirm(true)}
					>
						<Trash2 className="size-4" />
						Delete Study
					</Button>
				</div>
			) : (
				<div className="flex flex-col gap-3">
					<p className="text-sm text-foreground">
						Are you sure you want to delete <strong>{study.name}</strong>? This
						action cannot be undone.
					</p>
					<div className="flex items-center gap-2">
						<Button
							variant="destructive"
							size="sm"
							onClick={handleDelete}
							disabled={deleteStudy.isPending}
						>
							{deleteStudy.isPending ? "Deleting..." : "Confirm Delete"}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setShowConfirm(false)}
						>
							Cancel
						</Button>
					</div>
				</div>
			)}
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
