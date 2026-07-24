import { createFileRoute, Outlet } from "@tanstack/react-router";
import { ErrorBoundary } from "#/components/error-boundary";
import { AppHeader } from "#/components/shell/app-header";
import { AppSidebar } from "#/components/shell/app-sidebar";
import { EnvironmentBanner } from "#/components/shell/environment-banner";
import { UserProvider } from "#/lib/auth";
import { EnvironmentProvider } from "#/lib/environment";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";

export const Route = createFileRoute("/_app")({
	component: AppLayout,
});

function AppLayout() {
	return (
		<UserProvider>
			<EnvironmentProvider>
				<TooltipProvider>
					<SidebarProvider>
						<AppSidebar />
						<SidebarInset>
							<AppHeader />
							<div className="flex-1 overflow-auto">
								<EnvironmentBanner />
								<div className="p-6">
									<ErrorBoundary>
										<Outlet />
									</ErrorBoundary>
								</div>
							</div>
						</SidebarInset>
					</SidebarProvider>
				</TooltipProvider>
			</EnvironmentProvider>
		</UserProvider>
	);
}
