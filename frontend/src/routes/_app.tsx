import { createFileRoute, Outlet } from "@tanstack/react-router";
import { ErrorBoundary } from "#/components/error-boundary";
import { AppHeader } from "#/components/shell/app-header";
import { AppSidebar } from "#/components/shell/app-sidebar";
import { UserProvider } from "#/lib/auth";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";

export const Route = createFileRoute("/_app")({
	component: AppLayout,
});

function AppLayout() {
	return (
		<UserProvider>
			<TooltipProvider>
				<SidebarProvider>
					<AppSidebar />
					<SidebarInset>
						<AppHeader />
						<div className="flex-1 overflow-auto p-6">
							<ErrorBoundary>
								<Outlet />
							</ErrorBoundary>
						</div>
					</SidebarInset>
				</SidebarProvider>
			</TooltipProvider>
		</UserProvider>
	);
}
