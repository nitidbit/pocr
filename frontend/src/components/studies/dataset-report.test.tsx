import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DatasetField, DatasetReport } from "#/api/datasets";
import { demographicsReport } from "#/mocks/fixtures";
import { DatasetReportCard } from "./dataset-report";

const report: DatasetReport = {
	title: "Outcomes",
	description: "By arm",
	smd: false,
	arms: [
		{ id: "a", name: "Treatment", count: 10 },
		{ id: "b", name: "Control", count: 0 },
	],
	fields: [
		{
			analysis: "categorical",
			id: "readmission",
			name: "Readmission",
			categories: [
				{ id: "true", label: "Yes", counts: [2, 0] },
				{ id: null, label: "Missing", counts: [1, 0] },
			],
		},
		{
			analysis: "continuous",
			id: "los",
			name: "Mean LOS",
			unit: "days",
			note: null,
			decimals: 1,
			means: [4.2, null],
			standardDeviations: [1.3, null],
			denominators: [10, 0],
		},
		{
			analysis: "continuous",
			id: "days_to_return_to_ed",
			name: "Days to Return to ED",
			unit: "days",
			note: "ED returns with a recorded interval only",
			decimals: 1,
			means: [3, null],
			standardDeviations: [null, null],
			denominators: [4, 0],
		},
	],
};

describe("DatasetReportCard", () => {
	it("shows an empty snapshot state when no arm has rows", () => {
		render(
			<DatasetReportCard
				report={{
					...report,
					arms: report.arms.map((arm) => ({ ...arm, count: 0 })),
				}}
			/>,
		);
		expect(
			screen.getByText("No records in this snapshot."),
		).toBeInTheDocument();
	});

	it("renders approved notation and muted parentheticals", () => {
		render(<DatasetReportCard report={report} />);
		expect(screen.getByText("Readmission, n (%)")).toBeInTheDocument();
		expect(
			within(screen.getByRole("row", { name: /Mean LOS/ })).getByText("(1.3)"),
		).toBeInTheDocument();
		expect(
			within(screen.getByRole("row", { name: /^Yes/ })).getByText("(20.0)"),
		).toBeInTheDocument();
		expect(
			screen.getByText("ED returns with a recorded interval only"),
		).toBeInTheDocument();
	});

	it("renders server labels verbatim", () => {
		const days: DatasetField = {
			analysis: "categorical",
			id: "wait",
			name: "Days to Appointment",
			categories: [{ id: "a", label: "Days to Mean", counts: [1, 0] }],
		};
		render(
			<DatasetReportCard
				report={{ ...report, fields: [...report.fields, days] }}
			/>,
		);
		expect(screen.getByText("Days to Appointment, n (%)")).toBeInTheDocument();
		expect(screen.getByText("Mean LOS, mean (SD), days")).toBeInTheDocument();
		expect(
			screen.getByText("Days to Return to ED, mean (SD), days"),
		).toBeInTheDocument();
	});

	it("renders a column for a configured arm with no rows", () => {
		render(<DatasetReportCard report={report} />);
		expect(
			screen.getByRole("columnheader", { name: /Control/ }),
		).toHaveTextContent("n = 0 records");
	});

	it("renders a real 'missing' value and the null category as separate rows", () => {
		const field: DatasetField = {
			analysis: "categorical",
			id: "status",
			name: "Status",
			categories: [
				{ id: "missing", label: "missing", counts: [3, 0] },
				{ id: null, label: "Missing", counts: [1, 0] },
			],
		};
		render(<DatasetReportCard report={{ ...report, fields: [field] }} />);
		expect(screen.getByText("missing")).toBeInTheDocument();
		expect(screen.getByText("Missing")).toBeInTheDocument();
	});

	it("shows a conditional denominator only when it differs from the arm count", () => {
		render(<DatasetReportCard report={report} />);
		expect(screen.getByText("n = 4")).toBeInTheDocument();
		expect(screen.queryByText("n = 10")).not.toBeInTheDocument();
		expect(screen.queryByText("n = 0")).not.toBeInTheDocument();
	});

	it("names each report region by its own title", () => {
		render(
			<>
				<DatasetReportCard report={report} />
				<DatasetReportCard report={{ ...report, title: "Utilization" }} />
			</>,
		);
		expect(
			screen.getByRole("region", { name: "Outcomes" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("region", { name: "Utilization" }),
		).toBeInTheDocument();
	});

	it("renders <5 without a percentage for a suppressed category", () => {
		const field: DatasetField = {
			analysis: "categorical",
			id: "race",
			name: "Race",
			categories: [{ id: "a", label: "Small group", counts: [null, 0] }],
		};
		render(<DatasetReportCard report={{ ...report, fields: [field] }} />);
		const row = screen.getByRole("row", { name: /Small group/ });
		expect(within(row).getAllByRole("cell")[0]).toHaveTextContent(/^<5$/);
	});

	it("renders <5 for a suppressed continuous arm and omits its n line", () => {
		const field: DatasetField = {
			analysis: "continuous",
			id: "age",
			name: "Age",
			unit: null,
			note: null,
			decimals: 1,
			means: [null, 5],
			standardDeviations: [null, 1],
			denominators: [null, 0],
		};
		render(<DatasetReportCard report={{ ...report, fields: [field] }} />);
		const row = screen.getByRole("row", { name: /Age/ });
		expect(within(row).getByText("<5")).toBeInTheDocument();
		expect(within(row).queryByText(/n = /)).not.toBeInTheDocument();
	});

	it("shows no SMD column when the report disables SMD", () => {
		render(<DatasetReportCard report={report} />);
		expect(
			screen.queryByRole("columnheader", { name: "SMD" }),
		).not.toBeInTheDocument();
	});

	it("shows each field's SMD to two decimals when the report enables SMD", () => {
		render(<DatasetReportCard report={demographicsReport()} />);
		expect(screen.getByRole("columnheader", { name: "SMD" })).toBeVisible();
		expect(
			within(screen.getByRole("row", { name: /^Age/ })).getByText("0.03"),
		).toBeInTheDocument();
		expect(
			within(screen.getByRole("row", { name: /^Race/ })).getByText("0.18"),
		).toBeInTheDocument();
	});

	it("renders a dash for a null SMD", () => {
		render(<DatasetReportCard report={demographicsReport()} />);
		expect(
			within(screen.getByRole("row", { name: /^BMI/ })).getByText("—"),
		).toBeInTheDocument();
	});

	it("marks an imbalanced field", () => {
		render(<DatasetReportCard report={demographicsReport()} />);
		expect(
			within(screen.getByRole("row", { name: /^Race/ })).getByRole("img", {
				name: "Imbalanced",
			}),
		).toBeInTheDocument();
		expect(
			within(screen.getByRole("row", { name: /^Age/ })).queryByRole("img"),
		).not.toBeInTheDocument();
	});

	it("explains suppression only when a cell is suppressed", () => {
		const { unmount } = render(<DatasetReportCard report={report} />);
		expect(screen.queryByText(/suppressed/)).not.toBeInTheDocument();
		unmount();
		render(<DatasetReportCard report={demographicsReport()} />);
		expect(screen.getByText(/suppressed to protect privacy/)).toBeVisible();
		expect(screen.getByText(/SMD: largest absolute/)).toBeVisible();
	});
});
