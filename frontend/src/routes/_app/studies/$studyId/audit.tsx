import { createFileRoute } from "@tanstack/react-router";
import { AlertCircle, History, RefreshCw } from "lucide-react";
import type { AuditEvent, StudyResponse } from "#/api/client";
import { useAudit, useStudy } from "#/api/queries";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/studies/$studyId/audit")({
	component: StudyAuditPage,
});

function StudyAuditPage() {
	const { studyId } = Route.useParams();
	const { data: study } = useStudy(studyId);
	const { data: events, isLoading, isError, refetch } = useAudit(studyId);

	if (isLoading) {
		return (
			<div className="flex flex-col gap-3">
				<Skeleton className="h-16 rounded-lg" />
				<Skeleton className="h-16 rounded-lg" />
				<Skeleton className="h-16 rounded-lg" />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<AlertCircle className="size-8 text-destructive" />
				<h2 className="text-lg font-semibold">Failed to load history</h2>
				<Button variant="outline" onClick={() => refetch()}>
					<RefreshCw className="size-4" />
					Try again
				</Button>
			</div>
		);
	}

	if (!events || events.length === 0) {
		return (
			<section className="rounded-lg border bg-card p-5">
				<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
					<History className="size-4" />
					History
				</h2>
				<div className="flex h-32 items-center justify-center rounded-md border border-dashed bg-muted/30">
					<p className="text-sm text-muted-foreground">
						No activity recorded yet.
					</p>
				</div>
			</section>
		);
	}

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<History className="size-4" />
				History ({events.length})
			</h2>
			<ol className="flex flex-col gap-3">
				{events.map((event) => (
					<AuditRow key={event.id} event={event} study={study} />
				))}
			</ol>
		</section>
	);
}

interface AuditRowProps {
	event: AuditEvent;
	study: StudyResponse | undefined;
}

function AuditRow({ event, study }: AuditRowProps) {
	return (
		<li className="flex items-start justify-between gap-4 border-b pb-3 last:border-b-0 last:pb-0">
			<div className="flex flex-col gap-0.5">
				<p className="text-sm text-foreground">{describeEvent(event, study)}</p>
				<p className="text-xs text-muted-foreground">{event.actor}</p>
			</div>
			<time className="shrink-0 text-xs tabular-nums text-muted-foreground">
				{new Date(event.createdAt).toLocaleString()}
			</time>
		</li>
	);
}

/**
 * Render an audit event as a human-readable sentence.
 *
 * Arm identifiers in the event detail are resolved to arm names via the study
 * config when available, falling back to a truncated id. Unknown event types
 * degrade to their raw `eventType` string so new backend events still render.
 */
export function describeEvent(
	event: AuditEvent,
	study: StudyResponse | undefined,
): string {
	const detail = event.detail;
	switch (event.eventType) {
		case "study.transitioned":
			return `Status changed from ${asText(detail.from)} to ${asText(detail.to)}`;
		case "study.locator_set":
			return `Mapped ${armLabel(detail.armId, study)} to Epic locator ${asText(detail.locator)}`;
		case "study.ratio_changed":
			return `Changed ${armLabel(detail.armId, study)} allocation ratio from ${asText(detail.from)} to ${asText(detail.to)}`;
		case "study.nonproduction_purged":
			return `Purged ${asText(detail.recordsRemoved)} non-production randomizations`;
		case "study.deleted":
			return "Study deleted";
		default:
			return event.eventType;
	}
}

function armLabel(armId: unknown, study: StudyResponse | undefined): string {
	if (typeof armId !== "string") return "an arm";
	const arm = study?.randomizationConfig.arms.find((a) => a.id === armId);
	return arm?.name?.trim() ? arm.name : `arm ${armId}`;
}

function asText(value: unknown): string {
	if (value === null || value === undefined) return "—";
	return String(value);
}
