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
import { Route as StudyDetailRoute } from "./$studyId";

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyDetailLayout = StudyDetailRoute.options.component!;
const TEST_STUDY = MOCK_STUDIES[0];

function renderStudyDetail(studyId = TEST_STUDY.id) {
	const rootRoute = createRootRoute();

	const studyRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId",
		component: StudyDetailLayout,
	});

	const studyIndexRoute = createRoute({
		getParentRoute: () => studyRoute,
		path: "/",
		component: () => <div>Overview content</div>,
	});

	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([
			studyRoute.addChildren([studyIndexRoute]),
			catchAllRoute,
		]),
		history: createMemoryHistory({
			initialEntries: [`/studies/${studyId}`],
		}),
	});

	return render(<RouterProvider router={router} />);
}

describe("StudyDetailLayout", () => {
	it("renders the study title as page heading", async () => {
		renderStudyDetail();

		expect(
			await screen.findByRole("heading", { name: TEST_STUDY.name }),
		).toBeInTheDocument();
	});

	it("renders tab navigation with Overview, Configuration, and Monitoring", async () => {
		renderStudyDetail();

		const nav = await screen.findByRole("navigation", {
			name: /study detail/i,
		});
		expect(nav).toBeInTheDocument();

		expect(screen.getByText("Overview")).toBeInTheDocument();
		expect(screen.getByText("Configuration")).toBeInTheDocument();
		expect(screen.getByText("Monitoring")).toBeInTheDocument();
	});

	it("renders status badge for the study", async () => {
		renderStudyDetail();

		await waitFor(() => {
			expect(screen.getByText("Active")).toBeInTheDocument();
		});
	});

	it("renders operational status badge when present", async () => {
		renderStudyDetail();

		await waitFor(() => {
			expect(screen.getByText("running")).toBeInTheDocument();
		});
	});

	it("renders a Back to Studies link", async () => {
		renderStudyDetail();

		const link = await screen.findByRole("link", {
			name: /back to studies/i,
		});
		expect(link).toHaveAttribute("href", "/studies");
	});

	it("renders Study Not Found when study ID does not exist", async () => {
		renderStudyDetail("nonexistent-id");

		expect(
			await screen.findByRole("heading", { name: /study not found/i }),
		).toBeInTheDocument();
		expect(screen.getByText(/nonexistent-id/)).toBeInTheDocument();
	});
});
