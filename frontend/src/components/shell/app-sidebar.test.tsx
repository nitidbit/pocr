import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppSidebar } from "./app-sidebar";

const NAV_SECTIONS = [
	{ label: "Studies", path: "/studies" },
	{ label: "Strata", path: "/strata" },
	{ label: "Context", path: "/context" },
	{ label: "Reports", path: "/reports" },
	{ label: "Users", path: "/users" },
] as const;

function renderSidebar(initialPath = "/studies") {
	const rootRoute = createRootRoute({
		component: () => (
			<TooltipProvider>
				<SidebarProvider>
					<AppSidebar />
				</SidebarProvider>
			</TooltipProvider>
		),
	});

	const indexRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/",
	});

	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([indexRoute, catchAllRoute]),
		history: createMemoryHistory({ initialEntries: [initialPath] }),
	});

	return render(<RouterProvider router={router} />);
}

describe("AppSidebar", () => {
	it("renders all navigation section links", async () => {
		renderSidebar();

		for (const { label } of NAV_SECTIONS) {
			expect(await screen.findByText(label)).toBeInTheDocument();
		}
	});

	it("renders links pointing to correct routes", async () => {
		renderSidebar();

		await waitFor(() => {
			for (const { label, path } of NAV_SECTIONS) {
				const link = screen.getByRole("link", {
					name: new RegExp(label),
					hidden: true,
				});
				expect(link).toHaveAttribute("href", path);
			}
		});
	});

	it("renders the POCR branding in the sidebar header", async () => {
		renderSidebar();

		expect(await screen.findByText("POCR")).toBeInTheDocument();
		expect(screen.getByText("Randomization")).toBeInTheDocument();
	});

	it("renders a Settings item in the footer", async () => {
		renderSidebar();

		expect(await screen.findByText("Settings")).toBeInTheDocument();
	});
});
