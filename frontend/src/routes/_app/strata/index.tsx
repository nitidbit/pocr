import { createFileRoute } from "@tanstack/react-router";
import { Layers } from "lucide-react";

export const Route = createFileRoute("/_app/strata/")({
	component: StrataPage,
});

function StrataPage() {
	return (
		<div className="flex flex-col items-center justify-center gap-4 py-16">
			<div className="flex size-12 items-center justify-center rounded-full bg-muted">
				<Layers className="size-6 text-muted-foreground" />
			</div>
			<h1 className="text-2xl font-semibold text-foreground">
				Strata Management
			</h1>
			<p className="max-w-md text-center text-sm text-muted-foreground">
				Define and manage stratification variables used across your
				randomization studies. Configure factor levels, validation rules, and
				FHIR path mappings.
			</p>
			<span className="rounded-full bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
				Coming soon
			</span>
		</div>
	);
}
