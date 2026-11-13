import { fireEvent, screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DatasetAttachment, DatasetList } from "#/api/datasets";
import { datasetAttachment, datasetList } from "#/mocks/fixtures";
import { apiUrl, renderWithProviders, setupMockServer } from "#/test/msw";
import { StudyDatasets } from "./study-datasets";

const server = setupMockServer();
const datasetsUrl = apiUrl("/studies/study/datasets");
const catalog = {
	name: "encounter-outcomes",
	title: "Encounter Outcomes",
	description: "Discharge outcomes",
};

function serveList(list: DatasetList) {
	server.use(http.get(datasetsUrl, () => HttpResponse.json(list)));
}

function serveAttachment(overrides: Partial<DatasetAttachment>) {
	serveList(datasetList({ attachments: [datasetAttachment(overrides)] }));
}

const { createObjectURL, revokeObjectURL } = URL;

afterEach(() => {
	vi.restoreAllMocks();
	Object.assign(URL, { createObjectURL, revokeObjectURL });
});

describe("StudyDatasets", () => {
	it("shows an empty state when nothing is attached", async () => {
		serveList(datasetList());
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(
			await screen.findByText("No datasets attached to this study."),
		).toBeInTheDocument();
		expect(screen.queryByText("Attach a dataset")).not.toBeInTheDocument();
	});

	it("tells owners when the catalog has nothing left to attach", async () => {
		serveList(datasetList({ canManage: true }));
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(
			await screen.findByText("No additional datasets available."),
		).toBeInTheDocument();
	});

	it("attaches a dataset and shows it after the list refetches", async () => {
		let attached: DatasetAttachment[] = [];
		const bodies: unknown[] = [];
		server.use(
			http.get(datasetsUrl, () =>
				HttpResponse.json(
					datasetList({
						canManage: true,
						catalog: [catalog],
						attachments: attached,
					}),
				),
			),
			http.post(datasetsUrl, async ({ request }) => {
				bodies.push(await request.json());
				attached = [datasetAttachment()];
				return new HttpResponse(null, { status: 204 });
			}),
		);
		renderWithProviders(<StudyDatasets studyId="study" />);
		fireEvent.change(await screen.findByLabelText("Dataset"), {
			target: { value: "encounter-outcomes" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Attach" }));
		expect(
			await screen.findByRole("heading", { name: "Encounter Outcomes" }),
		).toBeInTheDocument();
		expect(bodies).toEqual([{ name: "encounter-outcomes" }]);
	});

	it("detaches only after confirmation", async () => {
		let attachments = [datasetAttachment()];
		let detaches = 0;
		server.use(
			http.get(datasetsUrl, () =>
				HttpResponse.json(datasetList({ canManage: true, attachments })),
			),
			http.delete(`${datasetsUrl}/encounter-outcomes`, () => {
				detaches += 1;
				attachments = [];
				return new HttpResponse(null, { status: 204 });
			}),
		);
		renderWithProviders(<StudyDatasets studyId="study" />);
		fireEvent.click(await screen.findByRole("button", { name: "Detach" }));
		expect(detaches).toBe(0);
		fireEvent.click(screen.getByRole("button", { name: "Confirm Detach" }));
		expect(
			await screen.findByText("No datasets attached to this study."),
		).toBeInTheDocument();
		expect(detaches).toBe(1);
	});

	it("hides management controls from members", async () => {
		serveAttachment({});
		renderWithProviders(<StudyDatasets studyId="study" />);
		await screen.findByRole("heading", { name: "Encounter Outcomes" });
		expect(screen.queryByRole("button", { name: "Detach" })).toBeNull();
	});

	it("downloads the CSV for the selected environment", async () => {
		serveAttachment({});
		const downloadUrls: string[] = [];
		server.use(
			http.get(`${datasetsUrl}/encounter-outcomes/download`, ({ request }) => {
				downloadUrls.push(request.url);
				return new HttpResponse("randomization_id\r\n", {
					headers: { "Content-Type": "text/csv" },
				});
			}),
		);
		const createObjectURL = vi.fn(() => "blob:csv");
		Object.assign(URL, { createObjectURL, revokeObjectURL: vi.fn() });
		const filenames: string[] = [];
		vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
			this: HTMLAnchorElement,
		) {
			filenames.push(this.download);
		});
		renderWithProviders(<StudyDatasets studyId="study" />);
		fireEvent.click(
			await screen.findByRole("button", { name: "Download CSV" }),
		);
		await waitFor(() =>
			expect(filenames).toEqual(["encounter-outcomes-production.csv"]),
		);
		expect(
			downloadUrls.map((url) => new URL(url).searchParams.get("environment")),
		).toEqual(["production"]);
	});

	it("disables download until the first refresh publishes", async () => {
		serveAttachment({
			status: "pending",
			hasSnapshot: false,
			completedAt: null,
			rowCount: 0,
		});
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(
			await screen.findByText("Awaiting first refresh"),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Download CSV" })).toBeDisabled();
	});

	it("shows an in-progress refresh", async () => {
		serveAttachment({ status: "running" });
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(await screen.findByText(/Refresh in progress/)).toBeInTheDocument();
	});

	it("keeps the last successful download available after a failed refresh", async () => {
		serveAttachment({ status: "failed" });
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(await screen.findByText(/Refresh failed/)).toBeInTheDocument();
		expect(
			screen.getByText("Downloads contain the last successful snapshot."),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Download CSV" })).toBeEnabled();
	});

	it("explains an unavailable definition", async () => {
		serveAttachment({ definitionAvailable: false });
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(
			await screen.findByText(/Definition unavailable/),
		).toBeInTheDocument();
	});

	it("explains a changed definition once a snapshot exists", async () => {
		serveAttachment({ definitionChanged: true });
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(await screen.findByText(/Definition updated/)).toBeInTheDocument();
	});

	it("offers a retry when the list cannot load", async () => {
		server.use(
			http.get(datasetsUrl, () => new HttpResponse(null, { status: 500 })),
		);
		renderWithProviders(<StudyDatasets studyId="study" />);
		expect(await screen.findByRole("alert")).toHaveTextContent(
			"Could not load datasets.",
		);
	});
});
