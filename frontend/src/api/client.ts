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
	status?: "draft" | "active" | "inactive";
}

export interface StudyResponse {
	id: string;
	name: string;
	description: string;
	status: "draft" | "active" | "inactive";
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

export interface RandomizationResponse {
	id: string;
	studyId: string;
	unit: { kind: string; data: unknown };
	strata: { kind: string; data: unknown } | null;
	arm: { id: string; ratio: number; locator?: number };
	issuedAt: string;
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
	tls?: TlsStatus;
}

export interface ServiceHealth {
	status: "up" | "down";
	latencyMs: number;
	error?: string;
}

export interface TlsStatus {
	subject: string;
	issuer: string;
	notBefore: string;
	notAfter: string;
	daysRemaining: number;
	valid: boolean;
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
		setArmLocator: (id: string, armId: string, locator: number) =>
			request<StudyResponse>(`/studies/${id}/arms/${armId}/locator`, {
				method: "PUT",
				body: JSON.stringify({ locator }),
			}),
	},
	randomizations: {
		list: (studyId: string, params?: { limit?: number; offset?: number }) => {
			const search = new URLSearchParams();
			if (params?.limit) search.set("limit", String(params.limit));
			if (params?.offset) search.set("offset", String(params.offset));
			const qs = search.toString();
			return request<RandomizationResponse[]>(
				`/studies/${studyId}/randomizations${qs ? `?${qs}` : ""}`,
			);
		},
	},
	balance: {
		get: (studyId: string) =>
			request<BalanceResponse>(`/studies/${studyId}/balance`),
	},
	system: {
		status: () => request<SystemStatus>("/system/status"),
	},
};
