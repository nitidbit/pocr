import { type ReportIncompatibility, useDatasetReports } from "#/api/datasets";
import type { Environment } from "#/lib/environment";
import { formatDateTime } from "#/lib/format";
import { DatasetReportCard } from "./dataset-report";

const REASON_COPY: Record<ReportIncompatibility["reason"], string> = {
	unknownField: "is missing",
	incompatibleType: "has a different type",
};

function IncompatibleReportCard({
	title,
	incompatibility,
}: {
	title: string;
	incompatibility: ReportIncompatibility;
}) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="text-sm font-medium text-muted-foreground">{title}</h2>
			<output className="mt-2 block text-sm text-muted-foreground">
				This report cannot be shown for the current snapshot: field{" "}
				{incompatibility.field} {REASON_COPY[incompatibility.reason]}. It will
				appear after the next successful refresh.
			</output>
		</section>
	);
}

export function DatasetReports({
	studyId,
	environment,
}: {
	studyId: string;
	environment: Environment;
}) {
	const { data, isLoading, isError } = useDatasetReports(studyId, environment);
	if (isLoading)
		return (
			<p className="text-sm text-muted-foreground">
				Loading dataset reports...
			</p>
		);
	if (isError)
		return (
			<p role="alert" className="text-sm text-destructive">
				Could not load dataset reports.
			</p>
		);
	return (
		<>
			{data?.map((item) => (
				<div key={item.name}>
					{item.status !== "succeeded" && (
						<p className="mb-2 text-xs text-muted-foreground">
							Showing the last successful refresh from{" "}
							{formatDateTime(item.completedAt)}. Latest refresh: {item.status}.
						</p>
					)}
					{item.definitionChanged && (
						<p className="mb-2 text-xs text-muted-foreground">
							The dataset definition has changed. This report will update after
							the next successful refresh.
						</p>
					)}
					{item.state === "ready" ? (
						<DatasetReportCard report={item.report} />
					) : (
						<IncompatibleReportCard
							title={item.title}
							incompatibility={item.incompatibility}
						/>
					)}
				</div>
			))}
		</>
	);
}
