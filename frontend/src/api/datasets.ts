import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { type Environment, request, requestBlob } from "./client";
import { studyKeys } from "./queries";

export type DatasetStatus = "pending" | "running" | "succeeded" | "failed";

export interface DatasetCatalogItem {
	name: string;
	title: string;
	description: string;
}

export interface DatasetAttachment extends DatasetCatalogItem {
	id: string;
	status: DatasetStatus;
	completedAt: string | null;
	rowCount: number;
	hasSnapshot: boolean;
	definitionAvailable: boolean;
	definitionChanged: boolean;
	/** Whether the caller's role may download this dataset's CSV. */
	canDownload: boolean;
}

export interface DatasetList {
	catalog: DatasetCatalogItem[];
	attachments: DatasetAttachment[];
	canManage: boolean;
}

/** SMD of one variable; both keys are present exactly when the report's `smd` is true. */
export interface FieldBalance {
	/** Largest absolute pairwise SMD; `null` when no pair of arms yields one. */
	smd?: number | null;
	/** Whether `smd` exceeds 0.1; `null` when `smd` is `null`. */
	imbalanced?: boolean | null;
}

/** One category of a categorical field; `id: null` is the missing category. */
export interface DatasetCategory {
	/** The stored value; boolean fields use `"true"` and `"false"`. */
	id: string | null;
	label: string;
	/** Rows per arm; `null` marks a suppressed small cell. */
	counts: (number | null)[];
}

/** A categorical field's per-arm counts align with `DatasetReport.arms`. */
export interface CategoricalField extends FieldBalance {
	analysis: "categorical";
	id: string;
	name: string;
	categories: DatasetCategory[];
}

/** A continuous field's per-arm arrays align with `DatasetReport.arms`. */
export interface ContinuousField extends FieldBalance {
	analysis: "continuous";
	id: string;
	name: string;
	unit: string | null;
	note: string | null;
	decimals: number;
	means: (number | null)[];
	standardDeviations: (number | null)[];
	/** Eligible values per arm; `null` marks a suppressed arm (its mean and SD are `null`). */
	denominators: (number | null)[];
}

export type DatasetField = CategoricalField | ContinuousField;

export interface ReportArm {
	id: string;
	name: string;
	count: number;
}

export interface DatasetReport {
	title: string;
	description: string;
	/** Whether the report shows an SMD column. */
	smd: boolean;
	arms: ReportArm[];
	fields: DatasetField[];
}

/** The dataset field that stopped a report from binding to a snapshot. */
export interface ReportIncompatibility {
	field: string;
	reason: "unknownField" | "incompatibleType";
}

export interface PublishedReportBase {
	/** Report name; unique across the catalog. */
	name: string;
	/** Dataset the report summarizes. */
	dataset: string;
	snapshotId: string;
	status: DatasetStatus;
	completedAt: string;
	/** Whether the deployed dataset definition differs from the snapshot's. */
	definitionChanged: boolean;
}

/** An incompatible report carries no summary data. */
export type PublishedReport = PublishedReportBase &
	(
		| { state: "ready"; report: DatasetReport }
		| {
				state: "incompatible";
				title: string;
				incompatibility: ReportIncompatibility;
		  }
	);

const studyPath = (study: string) => `/studies/${encodeURIComponent(study)}`;

export function useDatasets(study: string, environment: Environment) {
	return useQuery({
		queryKey: studyKeys.datasets(study, environment),
		queryFn: () =>
			request<DatasetList>(
				`${studyPath(study)}/datasets?environment=${environment}`,
			),
	});
}

export function useDatasetReports(study: string, environment: Environment) {
	return useQuery({
		queryKey: studyKeys.datasetReports(study, environment),
		queryFn: () =>
			request<PublishedReport[]>(
				`${studyPath(study)}/dataset-reports?environment=${environment}`,
			),
	});
}

export function useAttachDataset(study: string) {
	const client = useQueryClient();
	return useMutation({
		mutationFn: (name: string) =>
			request<void>(`${studyPath(study)}/datasets`, {
				method: "POST",
				body: JSON.stringify({ name }),
			}),
		onSuccess: () =>
			client.invalidateQueries({ queryKey: studyKeys.datasetsAll(study) }),
		onError: () => toast.error("Could not attach dataset"),
	});
}

export function useDetachDataset(study: string) {
	const client = useQueryClient();
	return useMutation({
		mutationFn: (name: string) =>
			request<void>(
				`${studyPath(study)}/datasets/${encodeURIComponent(name)}`,
				{
					method: "DELETE",
				},
			),
		onSuccess: () =>
			client.invalidateQueries({ queryKey: studyKeys.datasetsAll(study) }),
		onError: () => toast.error("Could not detach dataset"),
	});
}

export function useDownloadDataset(study: string, environment: Environment) {
	return useMutation({
		mutationFn: async (name: string) => {
			const blob = await requestBlob(
				`${studyPath(study)}/datasets/${encodeURIComponent(name)}/download?environment=${environment}`,
			);
			const url = URL.createObjectURL(blob);
			const anchor = document.createElement("a");
			anchor.href = url;
			anchor.download = `${name}-${environment}.csv`;
			anchor.click();
			// Browsers may read the blob URL asynchronously after click().
			setTimeout(() => URL.revokeObjectURL(url), 1000);
		},
		onError: () => toast.error("Could not download dataset"),
	});
}
