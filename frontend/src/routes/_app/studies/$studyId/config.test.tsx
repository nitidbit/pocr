import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError, api, type StudyResponse } from "#/api/client";
import { Route as StudyConfigRoute } from "./config";

vi.mock("#/api/client", async () => {
	const actual =
		await vi.importActual<typeof import("#/api/client")>("#/api/client");
	return {
		...actual,
		api: {
			studies: {
				get: vi.fn(),
				updateRatios: vi.fn(),
				setArmLocator: vi.fn(),
			},
		},
	};
});

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyConfigPage = StudyConfigRoute.options.component!;

const STUDY: StudyResponse = {
	id: "study-cfg",
	name: "Ratio Test Study",
	description: "",
	status: "active",
	randomizationConfig: {
		strategy: "block",
		arms: [
			{ id: "arm-a", name: "Arm A", ratio: 1 },
			{ id: "arm-b", name: "Arm B", ratio: 1 },
		],
		keys: [],
		strata: [],
	},
	createdAt: "2024-01-01T00:00:00Z",
	updatedAt: "2024-01-01T00:00:00Z",
};

function renderConfig(study: StudyResponse = STUDY) {
	vi.mocked(api.studies.get).mockImplementation(async (id) => {
		if (id !== study.id) throw new ApiError(404, null);
		return study;
	});

	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
	});

	const rootRoute = createRootRoute();
	const configRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId/config",
		component: StudyConfigPage,
	});
	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([configRoute, catchAllRoute]),
		history: createMemoryHistory({
			initialEntries: [`/studies/${study.id}/config`],
		}),
	});

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("StudyConfigPage — editable ratios", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("keeps save actions hidden until a ratio changes", async () => {
		renderConfig();

		await screen.findByText("Treatment Arms");
		expect(
			screen.queryByRole("button", { name: /review changes/i }),
		).not.toBeInTheDocument();
	});

	it("requires a second confirmation before applying a ratio change", async () => {
		vi.mocked(api.studies.updateRatios).mockResolvedValue(STUDY);
		renderConfig();

		await screen.findByText("Treatment Arms");
		fireEvent.change(screen.getByLabelText(/ratio for arm Arm A/i), {
			target: { value: "3" },
		});

		fireEvent.click(screen.getByRole("button", { name: /review changes/i }));

		expect(screen.getByText(/future allocations only/i)).toBeInTheDocument();
		expect(api.studies.updateRatios).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: /apply ratios/i }));

		await waitFor(() => {
			expect(api.studies.updateRatios).toHaveBeenCalledWith("study-cfg", [
				{ armId: "arm-a", ratio: 3 },
			]);
		});
	});

	it("blocks applying when every arm ratio is zero", async () => {
		renderConfig();

		await screen.findByText("Treatment Arms");
		fireEvent.change(screen.getByLabelText(/ratio for arm Arm A/i), {
			target: { value: "0" },
		});
		fireEvent.change(screen.getByLabelText(/ratio for arm Arm B/i), {
			target: { value: "0" },
		});

		expect(
			screen.getByRole("button", { name: /review changes/i }),
		).toBeDisabled();
	});

	it("recomputes allocation percentages from the edited ratios", async () => {
		renderConfig();

		await screen.findByText("Treatment Arms");
		fireEvent.change(screen.getByLabelText(/ratio for arm Arm A/i), {
			target: { value: "3" },
		});

		// 3 : 1 → 75% and 25%
		expect(screen.getByText("75%")).toBeInTheDocument();
		expect(screen.getByText("25%")).toBeInTheDocument();
	});

	it("re-enters through Review changes after edits revert to unchanged", async () => {
		renderConfig();

		await screen.findByText("Treatment Arms");
		const armA = screen.getByLabelText(/ratio for arm Arm A/i);
		fireEvent.change(armA, { target: { value: "3" } });
		fireEvent.click(screen.getByRole("button", { name: /review changes/i }));
		expect(screen.getByText(/future allocations only/i)).toBeInTheDocument();

		// Revert to the original ratio: the confirm panel collapses.
		fireEvent.change(armA, { target: { value: "1" } });
		expect(
			screen.queryByText(/future allocations only/i),
		).not.toBeInTheDocument();

		// A fresh edit returns to "Review changes", not straight to confirm.
		fireEvent.change(armA, { target: { value: "2" } });
		expect(
			screen.getByRole("button", { name: /review changes/i }),
		).toBeInTheDocument();
		expect(
			screen.queryByText(/future allocations only/i),
		).not.toBeInTheDocument();
	});
});
