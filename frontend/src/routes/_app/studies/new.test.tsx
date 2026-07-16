import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Route as NewStudyRoute } from "./new";

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const CreateStudyPage = NewStudyRoute.options.component!;

function renderWizard(step = 1) {
	const rootRoute = createRootRoute();

	const wizardRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/new",
		validateSearch: (search: Record<string, unknown>) => ({
			step: Math.min(5, Math.max(1, Number(search.step) || 1)),
		}),
		component: CreateStudyPage,
	});

	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([wizardRoute, catchAllRoute]),
		history: createMemoryHistory({
			initialEntries: [`/studies/new?step=${step}`],
		}),
	});

	return render(<RouterProvider router={router} />);
}

describe("CreateStudyPage", () => {
	it("renders the page title", async () => {
		renderWizard();

		expect(
			await screen.findByRole("heading", { name: /create new study/i }),
		).toBeInTheDocument();
	});

	it("renders the Study Information section heading on step 1", async () => {
		renderWizard(1);

		expect(
			await screen.findByRole("heading", { name: /study information/i }),
		).toBeInTheDocument();
	});

	it("renders a Back to Studies link", async () => {
		renderWizard();

		const link = await screen.findByRole("link", {
			name: /back to studies/i,
		});
		expect(link).toBeInTheDocument();
	});

	it("renders the Continue button on step 1", async () => {
		renderWizard();

		expect(
			await screen.findByRole("button", { name: /continue/i }),
		).toBeInTheDocument();
	});
});
