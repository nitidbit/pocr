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

import { ApiError, api, type StudyResponse } from "#/api/client";
import { MOCK_STUDIES } from "#/lib/mocks/studies";
import { Route as StudyOverviewRoute } from "./index";

vi.mock("#/api/client", async () => {
	const actual =
		await vi.importActual<typeof import("#/api/client")>("#/api/client");
	return {
		...actual,
		api: {
			studies: {
				get: vi.fn(),
				transition: vi.fn(),
				delete: vi.fn(),
			},
		},
	};
});

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyOverviewPage = StudyOverviewRoute.options.component!;
const TEST_STUDY = MOCK_STUDIES[0];

function renderOverview(study: StudyResponse = TEST_STUDY) {
	vi.mocked(api.studies.get).mockImplementation(async (id) => {
		if (id !== study.id) throw new ApiError(404, null);
		return study;
	});

	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
	});

	const rootRoute = createRootRoute();
	const overviewRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId/",
		component: StudyOverviewPage,
	});
	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([overviewRoute, catchAllRoute]),
		history: createMemoryHistory({
			initialEntries: [`/studies/${study.id}`],
		}),
	});

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("StudyOverviewPage — Treatment Arms table", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders each arm name", async () => {
		renderOverview();

		for (const arm of TEST_STUDY.randomizationConfig.arms) {
			expect(await screen.findByText(arm.name)).toBeInTheDocument();
		}
	});

	it("does not render a Locator column", async () => {
		renderOverview();

		await screen.findByText("Treatment Arms");
		expect(screen.queryByText("Locator")).not.toBeInTheDocument();
	});

	it("falls back to the arm ID when the name is empty", async () => {
		const study: StudyResponse = {
			...TEST_STUDY,
			randomizationConfig: {
				...TEST_STUDY.randomizationConfig,
				arms: [{ id: "arm-unnamed", name: "", ratio: 1 }],
			},
		};

		renderOverview(study);

		expect(await screen.findByText("arm-unnamed")).toBeInTheDocument();
	});
});
