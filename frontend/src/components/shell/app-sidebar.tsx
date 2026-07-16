import { Link, useMatchRoute } from "@tanstack/react-router";
import {
	Activity,
	BarChart3,
	BookOpen,
	FlaskConical,
	Layers,
	Settings,
	Users,
} from "lucide-react";

import {
	Sidebar,
	SidebarContent,
	SidebarFooter,
	SidebarGroup,
	SidebarGroupContent,
	SidebarGroupLabel,
	SidebarHeader,
	SidebarMenu,
	SidebarMenuButton,
	SidebarMenuItem,
	SidebarRail,
} from "@/components/ui/sidebar";

interface NavItem {
	to: string;
	label: string;
	icon: typeof FlaskConical;
}

const NAV_ITEMS: NavItem[] = [
	{ to: "/studies", label: "Studies", icon: FlaskConical },
	{ to: "/strata", label: "Strata", icon: Layers },
	{ to: "/context", label: "Context", icon: BookOpen },
	{ to: "/reports", label: "Reports", icon: BarChart3 },
	{ to: "/users", label: "Users", icon: Users },
	{ to: "/system", label: "System", icon: Activity },
];

export function AppSidebar() {
	const matchRoute = useMatchRoute();

	return (
		<Sidebar collapsible="icon">
			<SidebarHeader>
				<SidebarMenu>
					<SidebarMenuItem>
						<SidebarMenuButton size="lg" tooltip="POCR">
							<div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
								<FlaskConical className="size-4" />
							</div>
							<div className="grid flex-1 text-left text-sm leading-tight">
								<span className="truncate font-semibold">POCR</span>
								<span className="truncate text-xs text-muted-foreground">
									Randomization
								</span>
							</div>
						</SidebarMenuButton>
					</SidebarMenuItem>
				</SidebarMenu>
			</SidebarHeader>
			<SidebarContent>
				<nav aria-label="Main">
					<SidebarGroup>
						<SidebarGroupLabel>Navigation</SidebarGroupLabel>
						<SidebarGroupContent>
							<SidebarMenu>
								{NAV_ITEMS.map((item) => {
									const isActive = !!matchRoute({
										to: item.to,
										fuzzy: true,
									});
									return (
										<SidebarMenuItem key={item.label}>
											<SidebarMenuButton
												isActive={isActive}
												tooltip={item.label}
												render={<Link to={item.to} />}
											>
												<item.icon />
												<span>{item.label}</span>
											</SidebarMenuButton>
										</SidebarMenuItem>
									);
								})}
							</SidebarMenu>
						</SidebarGroupContent>
					</SidebarGroup>
				</nav>
			</SidebarContent>
			<SidebarFooter>
				<SidebarMenu>
					<SidebarMenuItem>
						<SidebarMenuButton tooltip="Settings">
							<Settings />
							<span>Settings</span>
						</SidebarMenuButton>
					</SidebarMenuItem>
				</SidebarMenu>
			</SidebarFooter>
			<SidebarRail />
		</Sidebar>
	);
}
