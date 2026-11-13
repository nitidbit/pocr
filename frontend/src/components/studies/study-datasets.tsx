import { useState } from "react";
import {
	type DatasetStatus,
	useAttachDataset,
	useDatasets,
	useDetachDataset,
	useDownloadDataset,
} from "#/api/datasets";
import { useEnvironment } from "#/lib/environment";
import { formatDateTime } from "#/lib/format";
import { Button } from "@/components/ui/button";

const STATUS_LABELS: Record<DatasetStatus, string> = {
	pending: "Awaiting first refresh",
	running: "Refresh in progress",
	failed: "Refresh failed",
	succeeded: "Ready",
};

export function StudyDatasets({ studyId }: { studyId: string }) {
	const { environment } = useEnvironment();
	const { data, isLoading, isError, refetch } = useDatasets(
		studyId,
		environment,
	);
	const attach = useAttachDataset(studyId);
	const detach = useDetachDataset(studyId);
	const download = useDownloadDataset(studyId, environment);
	const [selected, setSelected] = useState("");
	const [confirm, setConfirm] = useState<string | null>(null);
	if (isLoading)
		return <p className="text-sm text-muted-foreground">Loading datasets...</p>;
	if (isError || !data)
		return (
			<div role="alert">
				<p>Could not load datasets.</p>
				<Button variant="outline" onClick={() => refetch()}>
					Try again
				</Button>
			</div>
		);
	const available = data.catalog.filter(
		(item) => !data.attachments.some((a) => a.name === item.name),
	);
	return (
		<div className="flex flex-col gap-6">
			{data.canManage && (
				<section className="rounded-lg border bg-card p-5">
					<h2 className="mb-3 text-sm font-medium">Attach a dataset</h2>
					{available.length ? (
						<div className="flex items-center gap-3">
							<select
								aria-label="Dataset"
								className="rounded-md border bg-background px-3 py-2 text-sm"
								value={selected}
								onChange={(event) => setSelected(event.target.value)}
							>
								<option value="">Select a dataset</option>
								{available.map((item) => (
									<option key={item.name} value={item.name}>
										{item.title}
									</option>
								))}
							</select>
							<Button
								disabled={!selected || attach.isPending}
								onClick={() =>
									attach.mutate(selected, { onSuccess: () => setSelected("") })
								}
							>
								Attach
							</Button>
						</div>
					) : (
						<p className="text-sm text-muted-foreground">
							No additional datasets available.
						</p>
					)}
				</section>
			)}
			{!data.attachments.length && (
				<p className="py-8 text-center text-sm text-muted-foreground">
					No datasets attached to this study.
				</p>
			)}
			{data.attachments.map((item) => (
				<section key={item.id} className="rounded-lg border bg-card p-5">
					<div className="flex items-start justify-between gap-4">
						<div>
							<h2 className="font-medium">{item.title}</h2>
							<p className="mt-1 text-sm text-muted-foreground">
								{item.description}
							</p>
							<p className="mt-3 text-xs text-muted-foreground">
								{STATUS_LABELS[item.status]}
								{item.completedAt && (
									<>
										{" "}
										· {item.rowCount.toLocaleString()} rows · Last refreshed{" "}
										{formatDateTime(item.completedAt)}
									</>
								)}
							</p>
							{!item.definitionAvailable && (
								<p className="mt-2 text-xs text-muted-foreground">
									Definition unavailable. Saved data remains downloadable.
								</p>
							)}
							{item.definitionChanged && item.hasSnapshot && (
								<p className="mt-2 text-xs text-muted-foreground">
									Definition updated. Showing the previous snapshot until the
									next successful refresh.
								</p>
							)}
							{item.status === "failed" && item.hasSnapshot && (
								<p className="mt-2 text-xs text-muted-foreground">
									Downloads contain the last successful snapshot.
								</p>
							)}
						</div>
						<div className="flex gap-2">
							<Button
								variant="outline"
								disabled={!item.hasSnapshot || download.isPending}
								onClick={() => download.mutate(item.name)}
							>
								{download.isPending && download.variables === item.name
									? "Downloading..."
									: "Download CSV"}
							</Button>
							{data.canManage && (
								<Button
									variant="ghost"
									disabled={detach.isPending}
									onClick={() => setConfirm(item.name)}
								>
									Detach
								</Button>
							)}
						</div>
					</div>
					{confirm === item.name && (
						<div className="mt-4 border-t pt-4">
							<p className="mb-3 text-sm">
								Detach {item.title} and delete its saved data in both
								environments?
							</p>
							<div className="flex gap-2">
								<Button
									variant="destructive"
									disabled={detach.isPending}
									onClick={() =>
										detach.mutate(item.name, {
											onSuccess: () => setConfirm(null),
										})
									}
								>
									Confirm Detach
								</Button>
								<Button
									variant="outline"
									disabled={detach.isPending}
									onClick={() => setConfirm(null)}
								>
									Cancel
								</Button>
							</div>
						</div>
					)}
				</section>
			))}
		</div>
	);
}
