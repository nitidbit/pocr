import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type AuditEvent, api, type StudyResponse } from "#/api/client";
import { Route as AuditRoute } from "./audit";

vi.mock("#/api/client", async () => {
	const actual =
		await vi.importActual<typeof import("#/api/client")>("#/api/client");
	return {
		...actual,
		api: {
			studies: {
				get: vi.fn(),
				audit: vi.fn(),
			},
		},
	};
});

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyAuditPage = AuditRoute.options.component!;

const STUDY: StudyResponse = {
	id: "study-1",
	name: "Test Study",
	description: "",
	status: "active",
	randomizationConfig: {
		strategy: "block",
		arms: [
			{ id: "arm-control", name: "Control", ratio: 1 },
			{ id: "arm-treatment", name: "Treatment", ratio: 2 },
		],
		keys: [],
		strata: [],
	},
	createdAt: "2026-01-01T00:00:00Z",
	updatedAt: "2026-01-01T00:00:00Z",
};

function auditEvent(overrides: Partial<AuditEvent>): AuditEvent {
	return {
		id: "event-1",
		studyId: "study-1",
		eventType: "study.transitioned",
		actor: "alice@ucsf.edu",
		detail: {},
		createdAt: "2026-03-01T12:00:00Z",
		...overrides,
	};
}

function renderAudit() {
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});

	const rootRoute = createRootRoute();
	const auditRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId/audit",
		component: StudyAuditPage,
	});
	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([auditRoute, catchAllRoute]),
		history: createMemoryHistory({
			initialEntries: ["/studies/study-1/audit"],
		}),
	});

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("StudyAuditPage", () => {
	beforeEach(() => {
		vi.mocked(api.studies.get).mockResolvedValue(STUDY);
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it("shows an empty state when no events exist", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([]);
		renderAudit();

		expect(
			await screen.findByText(/no activity recorded yet/i),
		).toBeInTheDocument();
	});

	it("describes a transition event with its from and to states", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({
				eventType: "study.transitioned",
				detail: { from: "draft", to: "active" },
			}),
		]);
		renderAudit();

		expect(
			await screen.findByText(/status changed from draft to active/i),
		).toBeInTheDocument();
		expect(screen.getByText("alice@ucsf.edu")).toBeInTheDocument();
	});

	it("resolves an arm id to its name for a locator event", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({
				eventType: "study.locator_set",
				detail: { armId: "arm-treatment", locator: 42 },
			}),
		]);
		renderAudit();

		expect(
			await screen.findByText(/mapped treatment to epic locator 42/i),
		).toBeInTheDocument();
	});

	it("falls back to the raw event type for an unknown event", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({ eventType: "study.something_new", detail: {} }),
		]);
		renderAudit();

		expect(await screen.findByText("study.something_new")).toBeInTheDocument();
	});

	it("describes a non-production purge event", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({
				eventType: "study.nonproduction_purged",
				detail: { recordsRemoved: 12, blocksRemoved: 3 },
			}),
		]);
		renderAudit();

		expect(
			await screen.findByText(/purged 12 non-production randomizations/i),
		).toBeInTheDocument();
	});

	it("describes a study deletion event", async () => {
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({ eventType: "study.deleted", detail: {} }),
		]);
		renderAudit();

		expect(await screen.findByText("Study deleted")).toBeInTheDocument();
	});

	it("falls back to the full arm id when the arm name is empty", async () => {
		vi.mocked(api.studies.get).mockResolvedValue({
			...STUDY,
			randomizationConfig: {
				...STUDY.randomizationConfig,
				arms: [{ id: "arm-blank", name: "", ratio: 1 }],
			},
		});
		vi.mocked(api.studies.audit).mockResolvedValue([
			auditEvent({
				eventType: "study.locator_set",
				detail: { armId: "arm-blank", locator: 7 },
			}),
		]);
		renderAudit();

		expect(
			await screen.findByText(/mapped arm arm-blank to epic locator 7/i),
		).toBeInTheDocument();
	});
});
