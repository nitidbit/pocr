import { FlaskConical } from "lucide-react";
import { useUser } from "#/lib/auth";
import {
	ENVIRONMENTS,
	type Environment,
	useEnvironment,
} from "#/lib/environment";
import { cn } from "#/lib/utils";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Separator } from "@/components/ui/separator";
import { SidebarTrigger } from "@/components/ui/sidebar";

function EnvironmentSwitcher() {
	const { environment, setEnvironment } = useEnvironment();
	const isNonProduction = environment === "nonproduction";

	return (
		<div className="relative">
			{isNonProduction && (
				<FlaskConical className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-amber-700 dark:text-amber-400" />
			)}
			<select
				aria-label="Data environment"
				value={environment}
				onChange={(e) => setEnvironment(e.target.value as Environment)}
				className={cn(
					"h-7 cursor-pointer rounded-md border pr-6 text-xs font-medium outline-none transition-colors focus-visible:ring-3 focus-visible:ring-ring/50",
					isNonProduction
						? "border-amber-300 bg-amber-100 pl-7 text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-300"
						: "border-border bg-background pl-2.5 text-muted-foreground hover:text-foreground",
				)}
			>
				{(Object.keys(ENVIRONMENTS) as Environment[]).map((env) => (
					<option key={env} value={env}>
						{ENVIRONMENTS[env].label}
					</option>
				))}
			</select>
		</div>
	);
}

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
			<EnvironmentSwitcher />
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
