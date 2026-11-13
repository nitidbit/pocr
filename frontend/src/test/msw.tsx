import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { setupServer } from "msw/node";
import type { ReactElement } from "react";
import { afterAll, afterEach, beforeAll } from "vitest";
import { API_BASE } from "#/api/client";
import { EnvironmentProvider } from "#/lib/environment";

export const apiUrl = (path: string) => `${API_BASE}${path}`;

/** Start an MSW server for the file; unmatched requests fail the test. */
export function setupMockServer() {
	const server = setupServer();
	beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
	afterEach(() => server.resetHandlers());
	afterAll(() => server.close());
	return server;
}

/** Render with a real `QueryClient` (no retries) and the environment provider. */
export function renderWithProviders(ui: ReactElement) {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return render(
		<QueryClientProvider client={client}>
			<EnvironmentProvider>{ui}</EnvironmentProvider>
		</QueryClientProvider>,
	);
}
