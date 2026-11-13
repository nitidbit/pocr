import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Route as DatasetsRoute } from "./datasets";

vi.mock("#/components/studies/study-datasets", () => ({
	StudyDatasets: ({ studyId }: { studyId: string }) => (
		<p>datasets for {studyId}</p>
	),
}));

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const DatasetsPage = DatasetsRoute.options.component!;

describe("datasets route", () => {
	it("renders the study datasets component for the route's study", async () => {
		const rootRoute = createRootRoute();
		const route = createRoute({
			getParentRoute: () => rootRoute,
			path: "/studies/$studyId/datasets",
			component: DatasetsPage,
		});
		const router = createRouter({
			routeTree: rootRoute.addChildren([route]),
			history: createMemoryHistory({
				initialEntries: ["/studies/study-1/datasets"],
			}),
		});
		render(<RouterProvider router={router} />);
		expect(await screen.findByText("datasets for study-1")).toBeInTheDocument();
	});
});
