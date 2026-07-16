import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { MOCK_STUDIES } from "#/lib/mocks/studies";
import { Route as StudiesRoute } from "./index";

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudiesDashboardPage = StudiesRoute.options.component!;

function renderStudiesDashboard() {
	const rootRoute = createRootRoute({
		component: StudiesDashboardPage,
	});

	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([catchAllRoute]),
		history: createMemoryHistory({ initialEntries: ["/"] }),
	});

	return render(<RouterProvider router={router} />);
}

describe("StudiesDashboardPage", () => {
	it("renders the page heading and description", async () => {
		renderStudiesDashboard();

		expect(
			await screen.findByRole("heading", { name: /studies/i, level: 1 }),
		).toBeInTheDocument();
		expect(
			screen.getByText(/manage randomization studies/i),
		).toBeInTheDocument();
	});

	it("renders stat cards with computed values from mock data", async () => {
		renderStudiesDashboard();

		await waitFor(() => {
			expect(screen.getByText("Active Studies")).toBeInTheDocument();
		});

		expect(screen.getByText("Total Randomized")).toBeInTheDocument();
		expect(screen.getByText("In Setup")).toBeInTheDocument();

		const completedCards = screen.getAllByText("Completed");
		const statLabel = completedCards.find(
			(el) =>
				el.tagName === "P" && el.classList.contains("text-muted-foreground"),
		);
		expect(statLabel).toBeInTheDocument();
	});

	it("renders the study table with all mock data entries", async () => {
		renderStudiesDashboard();

		await waitFor(() => {
			expect(screen.getByText(MOCK_STUDIES[0].name)).toBeInTheDocument();
		});

		for (const study of MOCK_STUDIES) {
			expect(screen.getByText(study.name)).toBeInTheDocument();
		}
	});

	it("renders table column headers", async () => {
		renderStudiesDashboard();

		await waitFor(() => {
			expect(screen.getByText("Title")).toBeInTheDocument();
		});

		expect(screen.getByText("IRB Number")).toBeInTheDocument();
		expect(screen.getByText("Status")).toBeInTheDocument();
		expect(screen.getByText("Progress")).toBeInTheDocument();
		expect(screen.getByText("Last Updated")).toBeInTheDocument();
	});

	it("renders a Create New Study button linking to /studies/new", async () => {
		renderStudiesDashboard();

		const link = await screen.findByRole("link", {
			name: /create new study/i,
		});
		expect(link.getAttribute("href")).toContain("/studies/new");
	});
});
