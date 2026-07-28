import { createFileRoute, Link, Outlet } from "@tanstack/react-router";
import { ArrowLeft } from "lucide-react";

import { useStudy } from "#/api/queries";
import { STATUS_COLORS, STATUS_LABELS } from "#/lib/constants/study-status";
import { Badge } from "@/components/ui/badge";

export const Route = createFileRoute("/_app/studies/$studyId")({
	component: StudyDetailLayout,
});

interface TabItem {
	to: string;
	label: string;
	exact: boolean;
}

const TABS: TabItem[] = [
	{ to: "/studies/$studyId", label: "Overview", exact: true },
	{ to: "/studies/$studyId/config", label: "Configuration", exact: true },
	{ to: "/studies/$studyId/monitor", label: "Monitoring", exact: true },
	{ to: "/studies/$studyId/access", label: "Access", exact: true },
	{ to: "/studies/$studyId/audit", label: "History", exact: true },
];

function StudyDetailLayout() {
	const { studyId } = Route.useParams();
	const { data: study, isLoading, isError } = useStudy(studyId);

	if (isLoading) {
		return (
			<div className="flex items-center justify-center py-16">
				<p className="text-sm text-muted-foreground">Loading study...</p>
			</div>
		);
	}

	if (isError || !study) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<h1 className="text-2xl font-semibold text-foreground">
					Study Not Found
				</h1>
				<p className="text-muted-foreground">
					No study exists with ID &ldquo;{studyId}&rdquo;.
				</p>
				<Link
					to="/studies"
					className="text-sm font-medium text-primary hover:underline"
				>
					Back to Studies
				</Link>
			</div>
		);
	}

	const statusKey = study.status as keyof typeof STATUS_COLORS;

	return (
		<div className="flex flex-col gap-6">
			<div className="flex flex-col gap-4">
				<Link
					to="/studies"
					className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
				>
					<ArrowLeft className="size-3.5" />
					Back to Studies
				</Link>

				<div className="flex items-start justify-between gap-4">
					<div className="flex flex-col gap-1">
						<div className="flex items-center gap-3">
							<h1 className="text-2xl font-semibold tracking-tight text-foreground">
								{study.name}
							</h1>
							{STATUS_COLORS[statusKey] && (
								<Badge className={STATUS_COLORS[statusKey]}>
									{STATUS_LABELS[statusKey]}
								</Badge>
							)}
						</div>
						<p className="text-sm text-muted-foreground">{study.id}</p>
					</div>
				</div>

				<nav
					className="flex gap-6 border-b"
					aria-label="Study detail navigation"
				>
					{TABS.map((tab) => (
						<Link
							key={tab.label}
							to={tab.to}
							params={{ studyId }}
							activeProps={{
								className:
									"border-b-2 border-primary text-foreground font-medium",
							}}
							inactiveProps={{
								className: "text-muted-foreground hover:text-foreground",
							}}
							activeOptions={{ exact: tab.exact }}
							className="pb-3 text-sm transition-colors"
						>
							{tab.label}
						</Link>
					))}
				</nav>
			</div>

			<Outlet />
		</div>
	);
}
