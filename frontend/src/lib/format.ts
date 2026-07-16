/** Format an ISO date string as a short US locale date (e.g., "Jun 23, 2026"). */
export function formatDate(dateString: string): string {
	const date = new Date(dateString);
	return date.toLocaleDateString("en-US", {
		year: "numeric",
		month: "short",
		day: "numeric",
	});
}
