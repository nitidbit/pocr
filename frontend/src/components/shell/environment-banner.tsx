import { AlertTriangle } from "lucide-react";
import { useEnvironment } from "#/lib/environment";

export function EnvironmentBanner() {
	const { environment } = useEnvironment();

	if (environment !== "nonproduction") return null;

	return (
		<output className="flex items-center justify-center gap-2 border-b border-amber-300 bg-amber-100 px-4 py-2 text-sm font-medium text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-300">
			<AlertTriangle className="size-4 shrink-0" />
			<span>
				Non-Production mode — you are viewing test data, fully isolated from
				production.
			</span>
		</output>
	);
}
