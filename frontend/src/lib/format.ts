/** Format an ISO date string as a short US locale date (e.g., "Jun 23, 2026"). */
export function formatDate(dateString: string): string {
	const date = new Date(dateString);
	return date.toLocaleDateString("en-US", {
		year: "numeric",
		month: "short",
		day: "numeric",
	});
}

/** Format an ISO timestamp as a short US locale date and time (e.g., "Jun 23, 2026, 2:30 PM"). */
export function formatDateTime(dateString: string): string {
	return new Date(dateString).toLocaleString("en-US", {
		dateStyle: "medium",
		timeStyle: "short",
	});
}
