import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppHeader } from "./app-header";

function renderHeader() {
	return render(
		<TooltipProvider>
			<SidebarProvider>
				<AppHeader />
			</SidebarProvider>
		</TooltipProvider>,
	);
}

describe("AppHeader", () => {
	it("renders the application title", () => {
		renderHeader();

		expect(
			screen.getByText(/point-of-care randomization/i),
		).toBeInTheDocument();
	});

	it("renders the sidebar toggle button", () => {
		renderHeader();

		expect(
			screen.getByRole("button", { name: /toggle sidebar/i }),
		).toBeInTheDocument();
	});
});
