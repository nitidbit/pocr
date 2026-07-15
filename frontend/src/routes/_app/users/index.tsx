import { createFileRoute } from "@tanstack/react-router";
import { Users } from "lucide-react";

export const Route = createFileRoute("/_app/users/")({
	component: UsersPage,
});

function UsersPage() {
	return (
		<div className="flex flex-col items-center justify-center gap-4 py-16">
			<div className="flex size-12 items-center justify-center rounded-full bg-muted">
				<Users className="size-6 text-muted-foreground" />
			</div>
			<h1 className="text-2xl font-semibold text-foreground">
				User Management
			</h1>
			<p className="max-w-md text-center text-sm text-muted-foreground">
				Manage user accounts, assign roles and permissions, and configure site
				access. Control who can randomize, monitor, and administer studies.
			</p>
			<span className="rounded-full bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
				Coming soon
			</span>
		</div>
	);
}
