import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			staleTime: 30_000,
			retry: 1,
		},
	},
});

export function ApiProvider({ children }: { children: ReactNode }) {
	return (
		<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
	);
}
