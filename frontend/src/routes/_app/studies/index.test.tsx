import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { api } from "#/api/client";
import { MOCK_STUDIES } from "#/lib/mocks/studies";
import { Route as StudiesRoute } from "./index";

vi.mock("#/api/client", () => ({
	api: { studies: { list: vi.fn() } },
}));

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudiesDashboardPage = StudiesRoute.options.component!;

function renderStudiesDashboard() {
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});

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

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("StudiesDashboardPage", () => {
	beforeEach(() => {
		vi.mocked(api.studies.list).mockResolvedValue(MOCK_STUDIES);
	});

	it("renders the page heading and description", async () => {
		renderStudiesDashboard();

		expect(
			await screen.findByRole("heading", { name: /studies/i, level: 1 }),
		).toBeInTheDocument();
		expect(
			screen.getByText(/manage randomization studies/i),
		).toBeInTheDocument();
	});

	it("renders stat cards computed from the study list", async () => {
		renderStudiesDashboard();

		expect(await screen.findByText("Active Studies")).toBeInTheDocument();
		expect(screen.getByText("Total Studies")).toBeInTheDocument();
		expect(screen.getByText("In Draft")).toBeInTheDocument();
		// "Inactive" also appears as a status badge, so scope to the stat label.
		const inactiveLabel = screen
			.getAllByText("Inactive")
			.find((el) => el.classList.contains("text-muted-foreground"));
		expect(inactiveLabel).toBeInTheDocument();
	});

	it("renders a row for every study returned by the API", async () => {
		renderStudiesDashboard();

		expect(await screen.findByText(MOCK_STUDIES[0].name)).toBeInTheDocument();
		for (const study of MOCK_STUDIES) {
			expect(screen.getByText(study.name)).toBeInTheDocument();
		}
	});

	it("renders the study table column headers", async () => {
		renderStudiesDashboard();

		expect(await screen.findByText("Name")).toBeInTheDocument();
		expect(screen.getByText("Strategy")).toBeInTheDocument();
		expect(screen.getByText("Status")).toBeInTheDocument();
		expect(screen.getByText("Arms")).toBeInTheDocument();
		expect(screen.getByText("Last Updated")).toBeInTheDocument();
	});

	it("renders a Create New Study button linking to /studies/new", async () => {
		renderStudiesDashboard();

		const link = await screen.findByRole("link", {
			name: /create new study/i,
		});
		expect(link.getAttribute("href")).toContain("/studies/new");
	});

	it("renders the empty state when no studies exist", async () => {
		vi.mocked(api.studies.list).mockResolvedValue([]);
		renderStudiesDashboard();

		expect(await screen.findByText(/no studies yet/i)).toBeInTheDocument();
	});
});
