export type StudyStatus = "draft" | "active" | "inactive";

export const STATUS_LABELS: Record<StudyStatus, string> = {
	draft: "Draft",
	active: "Active",
	inactive: "Inactive",
};

export const STATUS_COLORS: Record<StudyStatus, string> = {
	draft: "bg-amber-100 text-amber-700",
	active: "bg-emerald-100 text-emerald-700",
	inactive: "bg-gray-100 text-gray-500",
};
