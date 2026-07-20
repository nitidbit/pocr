import { createFileRoute } from "@tanstack/react-router";
import {
	AlertCircle,
	CheckCircle2,
	Clock,
	Database,
	RefreshCw,
	Server,
	ShieldCheck,
} from "lucide-react";
import type { ServiceHealth, SystemStatus, TlsStatus } from "#/api/client";
import { useSystemStatus } from "#/api/queries";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/system/")({
	component: SystemStatusPage,
});

function SystemStatusPage() {
	const { data, isLoading, isError, refetch, dataUpdatedAt } =
		useSystemStatus();

	if (isLoading) {
		return (
			<div className="flex flex-col gap-6">
				<Skeleton className="h-8 w-48" />
				<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
					{["status", "uptime", "postgres", "redis"].map((metric) => (
						<Skeleton key={metric} className="h-28 rounded-lg" />
					))}
				</div>
				<Skeleton className="h-64 rounded-lg" />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<AlertCircle className="size-8 text-destructive" />
				<h2 className="text-lg font-semibold">Failed to load system status</h2>
				<Button variant="outline" onClick={() => refetch()}>
					<RefreshCw className="size-4" />
					Try again
				</Button>
			</div>
		);
	}

	if (!data) return null;

	return (
		<div className="flex flex-col gap-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold tracking-tight text-foreground">
						System Status
					</h1>
					<p className="text-sm text-muted-foreground">
						Infrastructure health, connectivity, and TLS certificate validity.
					</p>
				</div>
				<div className="flex items-center gap-3">
					{dataUpdatedAt > 0 && (
						<span className="text-xs text-muted-foreground">
							Updated {formatRelativeTime(dataUpdatedAt)}
						</span>
					)}
					<Button variant="outline" size="sm" onClick={() => refetch()}>
						<RefreshCw className="size-4" />
						Refresh
					</Button>
				</div>
			</div>

			<OverallStatusBanner status={data} />

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<UptimeCard uptimeSeconds={data.uptimeSeconds} />
				<ServiceCard
					label="PostgreSQL"
					icon={Database}
					health={data.services.postgres}
				/>
				<ServiceCard label="Redis" icon={Server} health={data.services.redis} />
				<TlsSummaryCard tls={data.tls} />
			</div>

			{data.tls && <TlsDetailSection tls={data.tls} />}
		</div>
	);
}

function OverallStatusBanner({ status }: { status: SystemStatus }) {
	const config = {
		healthy: {
			bg: "bg-emerald-50 border-emerald-200",
			text: "text-emerald-700",
			icon: CheckCircle2,
			label: "All Systems Operational",
		},
		degraded: {
			bg: "bg-amber-50 border-amber-200",
			text: "text-amber-700",
			icon: AlertCircle,
			label: "Degraded Performance",
		},
		unhealthy: {
			bg: "bg-red-50 border-red-200",
			text: "text-red-700",
			icon: AlertCircle,
			label: "System Unhealthy",
		},
	}[status.status];

	const Icon = config.icon;

	return (
		<div
			className={`flex items-center gap-3 rounded-lg border p-4 ${config.bg}`}
		>
			<Icon className={`size-5 ${config.text}`} />
			<span className={`text-sm font-medium ${config.text}`}>
				{config.label}
			</span>
			<span className="ml-auto text-xs text-muted-foreground">
				Server time: {new Date(status.serverTime).toLocaleString()}
			</span>
		</div>
	);
}

function UptimeCard({ uptimeSeconds }: { uptimeSeconds: number }) {
	return (
		<div className="rounded-lg border bg-card p-5">
			<div className="flex items-center gap-2">
				<Clock className="size-4 text-muted-foreground" />
				<p className="text-sm font-medium text-muted-foreground">Uptime</p>
			</div>
			<p className="mt-2 text-2xl font-semibold tabular-nums text-foreground">
				{formatUptime(uptimeSeconds)}
			</p>
		</div>
	);
}

function ServiceCard({
	label,
	icon: Icon,
	health,
}: {
	label: string;
	icon: typeof Database;
	health: ServiceHealth;
}) {
	const isUp = health.status === "up";

	return (
		<div className="rounded-lg border bg-card p-5">
			<div className="flex items-center justify-between">
				<div className="flex items-center gap-2">
					<Icon className="size-4 text-muted-foreground" />
					<p className="text-sm font-medium text-muted-foreground">{label}</p>
				</div>
				<Badge
					className={
						isUp ? "bg-emerald-100 text-emerald-700" : "bg-red-100 text-red-700"
					}
				>
					{isUp ? "Up" : "Down"}
				</Badge>
			</div>
			<p className="mt-2 text-2xl font-semibold tabular-nums text-foreground">
				{health.latencyMs}ms
			</p>
			{health.error && (
				<p className="mt-1 truncate text-xs text-destructive">{health.error}</p>
			)}
		</div>
	);
}

function TlsSummaryCard({ tls }: { tls?: TlsStatus }) {
	if (!tls) {
		return (
			<div className="rounded-lg border bg-card p-5">
				<div className="flex items-center gap-2">
					<ShieldCheck className="size-4 text-muted-foreground" />
					<p className="text-sm font-medium text-muted-foreground">
						TLS Certificate
					</p>
				</div>
				<p className="mt-2 text-sm text-muted-foreground">Not configured</p>
			</div>
		);
	}

	const urgency =
		tls.daysRemaining <= 7
			? "bg-red-100 text-red-700"
			: tls.daysRemaining <= 30
				? "bg-amber-100 text-amber-700"
				: "bg-emerald-100 text-emerald-700";

	return (
		<div className="rounded-lg border bg-card p-5">
			<div className="flex items-center justify-between">
				<div className="flex items-center gap-2">
					<ShieldCheck className="size-4 text-muted-foreground" />
					<p className="text-sm font-medium text-muted-foreground">
						TLS Certificate
					</p>
				</div>
				<Badge className={urgency}>
					{tls.valid ? `${tls.daysRemaining}d` : "Expired"}
				</Badge>
			</div>
			<p className="mt-2 text-2xl font-semibold tabular-nums text-foreground">
				{tls.daysRemaining}
			</p>
			<p className="text-xs text-muted-foreground">days remaining</p>
		</div>
	);
}

function TlsDetailSection({ tls }: { tls: TlsStatus }) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-4 text-sm font-medium text-muted-foreground">
				TLS Certificate Details
			</h2>
			<div className="grid gap-4 sm:grid-cols-2">
				<DetailRow label="Subject" value={tls.subject} />
				<DetailRow label="Issuer" value={tls.issuer} />
				<DetailRow
					label="Valid From"
					value={new Date(tls.notBefore).toLocaleDateString()}
				/>
				<DetailRow
					label="Expires"
					value={new Date(tls.notAfter).toLocaleDateString()}
				/>
				<DetailRow
					label="Days Remaining"
					value={String(tls.daysRemaining)}
					highlight={tls.daysRemaining <= 30}
				/>
				<DetailRow
					label="Status"
					value={tls.valid ? "Valid" : "Invalid / Expired"}
					highlight={!tls.valid}
				/>
			</div>
		</section>
	);
}

function DetailRow({
	label,
	value,
	highlight,
}: {
	label: string;
	value: string;
	highlight?: boolean;
}) {
	return (
		<div className="flex flex-col gap-0.5">
			<dt className="text-xs text-muted-foreground">{label}</dt>
			<dd
				className={`text-sm font-medium ${highlight ? "text-destructive" : "text-foreground"}`}
			>
				{value}
			</dd>
		</div>
	);
}

function formatUptime(seconds: number): string {
	const days = Math.floor(seconds / 86400);
	const hours = Math.floor((seconds % 86400) / 3600);
	const minutes = Math.floor((seconds % 3600) / 60);

	if (days > 0) return `${days}d ${hours}h`;
	if (hours > 0) return `${hours}h ${minutes}m`;
	return `${minutes}m`;
}

function formatRelativeTime(timestamp: number): string {
	const seconds = Math.floor((Date.now() - timestamp) / 1000);
	if (seconds < 5) return "just now";
	if (seconds < 60) return `${seconds}s ago`;
	return `${Math.floor(seconds / 60)}m ago`;
}
