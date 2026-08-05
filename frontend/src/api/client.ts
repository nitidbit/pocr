import type { Environment } from "#/lib/environment";

export type { Environment };

const API_BASE = import.meta.env.VITE_API_URL ?? "/api/v1";

export class ApiError extends Error {
	status: number;
	body: unknown;

	constructor(status: number, body: unknown) {
		super(`API error ${status}`);
		this.status = status;
		this.body = body;
	}
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
	const url = `${API_BASE}${path}`;
	const devHeaders: Record<string, string> = {};
	if (import.meta.env.DEV) {
		devHeaders["X-Vouch-User"] =
			import.meta.env.VITE_DEV_USER ?? "dev@localhost";
	}
	const response = await fetch(url, {
		headers: {
			"Content-Type": "application/json",
			...devHeaders,
			...options?.headers,
		},
		...options,
	});
	if (!response.ok) {
		throw new ApiError(
			response.status,
			await response.json().catch(() => null),
		);
	}
	if (response.status === 204) {
		return undefined as T;
	}
	return response.json();
}

export type KeyVariant = "patientId" | "encounterId" | "providerId";

export type StudyStatus = "draft" | "active" | "paused" | "closed";

export interface CreateStudyRequest {
	name: string;
	description: string;
	strategy: "simple" | "block";
	arms: Array<{ name: string; ratio: number }>;
	keys?: Array<KeyVariant | null>;
	strata?: Array<{ name: string; context: string; path: string }>;
	blockSize?: number;
}

export interface UpdateStudyRequest {
	name?: string;
	description?: string;
	// Status changes go through the transition endpoint, not update.
}

export interface ArmRatioUpdate {
	armId: string;
	ratio: number;
}

export interface StudyResponse {
	id: string;
	name: string;
	description: string;
	status: StudyStatus;
	randomizationConfig: {
		strategy: "simple" | "block";
		arms: Array<{ id: string; name: string; ratio: number; locator?: number }>;
		keys: Array<{ PatientId?: null; EncounterId?: null; ProviderId?: null }>;
		strata: Array<{ id: string; name: string; context: string; path: string }>;
		blockSize?: number;
	};
	createdAt: string;
	updatedAt: string;
}

export interface AuditEvent {
	id: string;
	studyId: string;
	eventType: string;
	actor: string;
	detail: Record<string, unknown>;
	createdAt: string;
}

export type StudyRole = "owner" | "member";

export interface StudyMember {
	userId: string;
	role: StudyRole;
	joinedAt: string;
}

export interface RandomizationResponse {
	id: string;
	studyId: string;
	environment: Environment;
	unit: { kind: string; data: unknown };
	strata: { kind: string; data: unknown } | null;
	arm: { id: string; name?: string; ratio: number; locator?: number };
	issuedAt: string;
}

export interface ArmCount {
	armId: string;
	count: number;
}

export interface RandomizationCounts {
	total: number;
	arms: ArmCount[];
}

export interface BalanceArmSummary {
	id: string;
	count: number;
}

export interface ContinuousArmData {
	arm_id: string;
	mean: number;
	sd: number;
	missing: number;
}

export interface CategoricalArmData {
	arm_id: string;
	count: number | string;
	pct: number | string;
}

export interface CategoryData {
	value: string;
	arms: CategoricalArmData[];
}

export type BalanceVariable =
	| {
			type: "continuous";
			name: string;
			arms: ContinuousArmData[];
			smd: number;
	  }
	| {
			type: "categorical";
			name: string;
			categories: CategoryData[];
			smd: number;
	  };

export interface BalanceResponse {
	study_id: string;
	total_patients: number;
	arms: BalanceArmSummary[];
	variables: BalanceVariable[];
}

export interface SystemStatus {
	status: "healthy" | "degraded" | "unhealthy";
	uptimeSeconds: number;
	serverTime: string;
	services: {
		postgres: ServiceHealth;
		redis: ServiceHealth;
	};
	tls: TlsStatus;
}

export interface ServiceHealth {
	status: "up" | "down";
	latencyMs: number;
	error?: string;
}

// TLS status is a discriminated union on `state`, mirroring the backend enum:
// no cert configured, a read/parse error, or a successfully-parsed chain.
export type TlsStatus =
	| { state: "notConfigured" }
	| { state: "error"; message: string }
	| ({ state: "ok" } & TlsInfo);

export interface TlsInfo {
	subject: string;
	issuer: string;
	notBefore: string;
	notAfter: string;
	daysRemaining: number;
	valid: boolean;
	// Soonest expiry reflects this many certs in the chain.
	chainLength: number;
}

export const api = {
	studies: {
		list: () => request<StudyResponse[]>("/studies"),
		get: (id: string) => request<StudyResponse>(`/studies/${id}`),
		create: (data: CreateStudyRequest) =>
			request<StudyResponse>("/studies", {
				method: "POST",
				body: JSON.stringify(data),
			}),
		update: (id: string, data: UpdateStudyRequest) =>
			request<StudyResponse>(`/studies/${id}`, {
				method: "PUT",
				body: JSON.stringify(data),
			}),
		delete: (id: string) =>
			request<void>(`/studies/${id}`, { method: "DELETE" }),
		transition: (id: string, status: StudyStatus) =>
			request<StudyResponse>(`/studies/${id}/transition`, {
				method: "POST",
				body: JSON.stringify({ status }),
			}),
		setArmLocator: (id: string, armId: string, locator: number) =>
			request<StudyResponse>(`/studies/${id}/arms/${armId}/locator`, {
				method: "PUT",
				body: JSON.stringify({ locator }),
			}),
		updateRatios: (id: string, arms: ArmRatioUpdate[]) =>
			request<StudyResponse>(`/studies/${id}/arms/ratios`, {
				method: "PUT",
				body: JSON.stringify({ arms }),
			}),
		purgeNonProduction: (id: string) =>
			request<{ recordsRemoved: number; blocksRemoved: number }>(
				`/studies/${id}/nonproduction`,
				{ method: "DELETE" },
			),
		audit: (id: string) => request<AuditEvent[]>(`/studies/${id}/audit`),
		members: {
			list: (id: string) => request<StudyMember[]>(`/studies/${id}/members`),
			add: (id: string, email: string, role: StudyRole) =>
				request<StudyMember>(`/studies/${id}/members`, {
					method: "POST",
					body: JSON.stringify({ email, role }),
				}),
			updateRole: (id: string, email: string, role: StudyRole) =>
				request<StudyMember>(
					`/studies/${id}/members/${encodeURIComponent(email)}`,
					{ method: "PUT", body: JSON.stringify({ role }) },
				),
			remove: (id: string, email: string) =>
				request<void>(`/studies/${id}/members/${encodeURIComponent(email)}`, {
					method: "DELETE",
				}),
		},
	},
	randomizations: {
		list: (
			studyId: string,
			params?: { environment?: Environment; limit?: number; offset?: number },
		) => {
			const search = new URLSearchParams();
			if (params?.environment) search.set("environment", params.environment);
			if (params?.limit) search.set("limit", String(params.limit));
			if (params?.offset) search.set("offset", String(params.offset));
			const qs = search.toString();
			return request<RandomizationResponse[]>(
				`/studies/${studyId}/randomizations${qs ? `?${qs}` : ""}`,
			);
		},
		// Whole-study per-arm tallies and total, aggregated server-side so they
		// are independent of the paginated `list` endpoint's row cap.
		counts: (studyId: string, environment?: Environment) => {
			const search = new URLSearchParams();
			if (environment) search.set("environment", environment);
			const qs = search.toString();
			return request<RandomizationCounts>(
				`/studies/${studyId}/randomizations/counts${qs ? `?${qs}` : ""}`,
			);
		},
	},
	balance: {
		// The backend balance endpoint currently ignores the environment param;
		// we thread it through defensively so it works once the backend adds it.
		get: (studyId: string, environment?: Environment) => {
			const search = new URLSearchParams();
			if (environment) search.set("environment", environment);
			const qs = search.toString();
			return request<BalanceResponse>(
				`/studies/${studyId}/balance${qs ? `?${qs}` : ""}`,
			);
		},
	},
	system: {
		status: () => request<SystemStatus>("/system/status"),
	},
};
