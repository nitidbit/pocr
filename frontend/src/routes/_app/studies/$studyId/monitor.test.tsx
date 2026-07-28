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

import type { RandomizationResponse } from "#/api/client";
import { MOCK_STUDIES } from "#/lib/mocks/studies";
import { armDisplayName, Route as MonitorRoute } from "./monitor";

vi.mock("#/api/queries", () => ({
	useStudy: vi.fn(),
	useRandomizations: vi.fn(),
	useBalance: vi.fn(),
	usePurgeNonProduction: vi.fn(),
}));

vi.mock("#/lib/environment", async () => {
	const actual =
		await vi.importActual<typeof import("#/lib/environment")>(
			"#/lib/environment",
		);
	return {
		...actual,
		useEnvironment: vi.fn(() => ({
			environment: "production" as const,
			setEnvironment: vi.fn(),
		})),
	};
});

import { useBalance, useRandomizations, useStudy } from "#/api/queries";

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyMonitorPage = MonitorRoute.options.component!;
const TEST_STUDY = MOCK_STUDIES[0];

const RANDOMIZATION_ID = "019fa785-1111-7abc-8def-0123456789ab";

function makeRandomization(
	arm: RandomizationResponse["arm"],
): RandomizationResponse {
	return {
		id: RANDOMIZATION_ID,
		studyId: TEST_STUDY.id,
		environment: "production",
		unit: { kind: "simple", data: {} },
		strata: null,
		arm,
		issuedAt: "2025-05-01T14:30:00Z",
	};
}

// biome-ignore lint/suspicious/noExplicitAny: minimal query-result stub
function stubQuery(data: unknown, isLoading = false): any {
	return { data, isLoading };
}

function renderMonitor(randomizations: RandomizationResponse[]) {
	vi.mocked(useStudy).mockReturnValue(stubQuery(TEST_STUDY));
	vi.mocked(useRandomizations).mockReturnValue(stubQuery(randomizations));
	vi.mocked(useBalance).mockReturnValue(stubQuery(undefined));

	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const rootRoute = createRootRoute();
	const monitorRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId/monitor",
		component: StudyMonitorPage,
	});
	const router = createRouter({
		routeTree: rootRoute.addChildren([monitorRoute]),
		history: createMemoryHistory({
			initialEntries: [`/studies/${TEST_STUDY.id}/monitor`],
		}),
	});

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("armDisplayName", () => {
	it("returns the name when present", () => {
		expect(armDisplayName({ id: "arm-001", name: "Treatment A" })).toBe(
			"Treatment A",
		);
	});

	it("falls back to the full id when the name is empty", () => {
		expect(armDisplayName({ id: "arm-001", name: "" })).toBe("arm-001");
	});

	it("falls back to the full id when the name is whitespace", () => {
		expect(armDisplayName({ id: "arm-001", name: "   " })).toBe("arm-001");
	});

	it("falls back to the full id when the name is absent", () => {
		expect(armDisplayName({ id: "arm-001" })).toBe("arm-001");
	});
});

describe("StudyMonitorPage", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders arm names in the balance summary, not truncated ids", async () => {
		renderMonitor([
			makeRandomization({
				id: "arm-001",
				name: "Goal-Directed Therapy",
				ratio: 1,
			}),
		]);

		expect(
			(await screen.findAllByText("Goal-Directed Therapy")).length,
		).toBeGreaterThan(0);
		expect(screen.queryByText("arm-001...")).not.toBeInTheDocument();
	});

	it("renders the full randomization id, not a truncated prefix", async () => {
		renderMonitor([
			makeRandomization({
				id: "arm-001",
				name: "Goal-Directed Therapy",
				ratio: 1,
			}),
		]);

		expect(await screen.findByText(RANDOMIZATION_ID)).toBeInTheDocument();
	});

	it("renders a valid issued-at date, not 'Invalid Date'", async () => {
		renderMonitor([
			makeRandomization({
				id: "arm-001",
				name: "Goal-Directed Therapy",
				ratio: 1,
			}),
		]);

		expect(
			await screen.findByText(
				new Date("2025-05-01T14:30:00Z").toLocaleString(),
			),
		).toBeInTheDocument();
		expect(screen.queryByText("Invalid Date")).not.toBeInTheDocument();
	});

	it("falls back to the full arm id when the randomization arm has no name", async () => {
		renderMonitor([makeRandomization({ id: "arm-001", name: "", ratio: 1 })]);

		expect(await screen.findByText("arm-001")).toBeInTheDocument();
	});
});
