import { useUser } from "#/lib/auth";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Separator } from "@/components/ui/separator";
import { SidebarTrigger } from "@/components/ui/sidebar";

export function AppHeader() {
	const user = useUser();

	const initials = user
		? user.name
				.split(" ")
				.map((n) => n[0])
				.join("")
				.toUpperCase()
				.slice(0, 2)
		: "";

	return (
		<header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
			<SidebarTrigger className="-ml-1" />
			<Separator orientation="vertical" className="mr-2 h-4" />
			<span className="text-sm font-medium">Point-of-Care Randomization</span>
			{user && (
				<div className="ml-auto flex items-center gap-3">
					<div className="text-right">
						<p className="text-sm font-medium leading-none">{user.name}</p>
						<p className="text-xs text-muted-foreground">{user.email}</p>
					</div>
					<Avatar className="h-8 w-8">
						<AvatarImage src={user.picture} alt={user.name} />
						<AvatarFallback className="text-xs">{initials}</AvatarFallback>
					</Avatar>
				</div>
			)}
		</header>
	);
}
