import { createFileRoute } from "@tanstack/react-router";
import { BarChart3 } from "lucide-react";

export const Route = createFileRoute("/_app/reports/")({
	component: ReportsPage,
});

function ReportsPage() {
	return (
		<div className="flex flex-col items-center justify-center gap-4 py-16">
			<div className="flex size-12 items-center justify-center rounded-full bg-muted">
				<BarChart3 className="size-6 text-muted-foreground" />
			</div>
			<h1 className="text-2xl font-semibold text-foreground">Reports</h1>
			<p className="max-w-md text-center text-sm text-muted-foreground">
				Generate enrollment reports, allocation balance summaries, and audit
				trail exports. Monitor study progress with customizable date ranges and
				site filters.
			</p>
			<span className="rounded-full bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
				Coming soon
			</span>
		</div>
	);
}
