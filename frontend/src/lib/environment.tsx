import {
	createContext,
	type ReactNode,
	use,
	useCallback,
	useState,
} from "react";

export type Environment = "production" | "nonproduction";

export const ENVIRONMENTS: Record<Environment, { label: string }> = {
	production: { label: "Production" },
	nonproduction: { label: "Non-Production" },
};

const STORAGE_KEY = "pocr.environment";
const DEFAULT_ENVIRONMENT: Environment = "production";

interface EnvironmentContextValue {
	environment: Environment;
	setEnvironment: (environment: Environment) => void;
}

const EnvironmentContext = createContext<EnvironmentContextValue | null>(null);

function isEnvironment(value: unknown): value is Environment {
	return value === "production" || value === "nonproduction";
}

function readStoredEnvironment(): Environment {
	if (typeof window === "undefined") return DEFAULT_ENVIRONMENT;
	try {
		const stored = window.localStorage.getItem(STORAGE_KEY);
		return isEnvironment(stored) ? stored : DEFAULT_ENVIRONMENT;
	} catch {
		return DEFAULT_ENVIRONMENT;
	}
}

export function EnvironmentProvider({ children }: { children: ReactNode }) {
	const [environment, setEnvironmentState] = useState<Environment>(
		readStoredEnvironment,
	);

	const setEnvironment = useCallback((next: Environment) => {
		setEnvironmentState(next);
		try {
			window.localStorage.setItem(STORAGE_KEY, next);
		} catch {
			// Persistence is best-effort; ignore storage failures.
		}
	}, []);

	return (
		<EnvironmentContext value={{ environment, setEnvironment }}>
			{children}
		</EnvironmentContext>
	);
}

export function useEnvironment(): EnvironmentContextValue {
	const context = use(EnvironmentContext);
	if (context === null) {
		throw new Error(
			"useEnvironment must be used within an EnvironmentProvider",
		);
	}
	return context;
}
