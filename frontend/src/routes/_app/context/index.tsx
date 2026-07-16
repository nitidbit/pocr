import { createFileRoute } from "@tanstack/react-router";
import { BookOpen } from "lucide-react";

export const Route = createFileRoute("/_app/context/")({
	component: ContextPage,
});

function ContextPage() {
	return (
		<div className="flex flex-col items-center justify-center gap-4 py-16">
			<div className="flex size-12 items-center justify-center rounded-full bg-muted">
				<BookOpen className="size-6 text-muted-foreground" />
			</div>
			<h1 className="text-2xl font-semibold text-foreground">
				Context Configuration
			</h1>
			<p className="max-w-md text-center text-sm text-muted-foreground">
				Configure the contextual data captured at randomization time. Map FHIR
				resources, define required fields, and set up validation rules for
				enrollment data.
			</p>
			<span className="rounded-full bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
				Coming soon
			</span>
		</div>
	);
}
