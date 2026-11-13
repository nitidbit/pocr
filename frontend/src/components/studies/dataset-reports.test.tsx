import { screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import type { PublishedReport } from "#/api/datasets";
import { incompatibleReport, publishedReport } from "#/mocks/fixtures";
import { apiUrl, renderWithProviders, setupMockServer } from "#/test/msw";
import { DatasetReports } from "./dataset-reports";

const server = setupMockServer();

function serveReports(reports: PublishedReport[]) {
	server.use(
		http.get(apiUrl("/studies/study/dataset-reports"), () =>
			HttpResponse.json(reports),
		),
	);
}

function renderReports() {
	return renderWithProviders(
		<DatasetReports studyId="study" environment="production" />,
	);
}

describe("DatasetReports", () => {
	it("renders the published report", async () => {
		serveReports([publishedReport()]);
		renderReports();
		expect(
			await screen.findByRole("heading", { name: "Encounter Outcomes" }),
		).toBeInTheDocument();
		expect(screen.getByText("LOS, mean (SD), days")).toBeInTheDocument();
	});

	it("identifies a changed definition even after a successful refresh", async () => {
		serveReports([publishedReport({ definitionChanged: true })]);
		renderReports();
		expect(
			await screen.findByText(/definition has changed/),
		).toBeInTheDocument();
	});

	it("renders an incompatible report without a table", async () => {
		serveReports([incompatibleReport()]);
		renderReports();
		expect(
			await screen.findByRole("heading", { name: "Readmissions" }),
		).toBeInTheDocument();
		expect(screen.getByRole("status")).toHaveTextContent(
			"field los is missing. It will appear after the next successful refresh.",
		);
		expect(screen.queryByRole("table")).not.toBeInTheDocument();
	});

	it("explains a type change distinctly", async () => {
		serveReports([
			incompatibleReport({
				incompatibility: { field: "los", reason: "incompatibleType" },
			}),
		]);
		renderReports();
		expect(await screen.findByRole("status")).toHaveTextContent(
			"field los has a different type.",
		);
	});

	it("renders every report of one dataset", async () => {
		serveReports([
			publishedReport(),
			incompatibleReport({ name: "readmissions" }),
		]);
		renderReports();
		expect(
			await screen.findByRole("heading", { name: "Encounter Outcomes" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("heading", { name: "Readmissions" }),
		).toBeInTheDocument();
	});

	it("notes when the latest refresh did not succeed", async () => {
		serveReports([publishedReport({ status: "failed" })]);
		renderReports();
		expect(
			await screen.findByText(/Showing the last successful refresh/),
		).toHaveTextContent("Latest refresh: failed.");
	});

	it("reports a load failure", async () => {
		server.use(
			http.get(
				apiUrl("/studies/study/dataset-reports"),
				() => new HttpResponse(null, { status: 500 }),
			),
		);
		renderReports();
		expect(await screen.findByRole("alert")).toHaveTextContent(
			"Could not load dataset reports.",
		);
	});
});
