// Mirrors the engine's StudyStatus state machine:
// Draft → Active → Paused ⇄ Active, and Draft/Active/Paused → Closed (terminal).
export type StudyStatus = "draft" | "active" | "paused" | "closed";

export const STATUS_LABELS: Record<StudyStatus, string> = {
	draft: "Draft",
	active: "Active",
	paused: "Paused",
	closed: "Closed",
};

export const STATUS_COLORS: Record<StudyStatus, string> = {
	draft: "bg-amber-100 text-amber-700",
	active: "bg-emerald-100 text-emerald-700",
	paused: "bg-blue-100 text-blue-700",
	closed: "bg-gray-100 text-gray-500",
};
