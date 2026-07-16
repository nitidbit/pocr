import { useQuery } from "@tanstack/react-query";
import { createContext, type ReactNode, use } from "react";

export interface User {
	email: string;
	name: string;
	givenName: string;
	picture: string;
	groups: string[];
}

const UserContext = createContext<User | null>(null);

const MOCK_USER: User = {
	email: "andrew.robinson@ucsf.edu",
	name: "Andrew Robinson",
	givenName: "Andrew",
	picture: "",
	groups: ["ctsi-randomization-admins"],
};

function useUserQuery() {
	return useQuery<User>({
		queryKey: ["user", "me"],
		queryFn: async () => {
			const res = await fetch("/api/me");
			if (!res.ok) throw new Error("Failed to fetch user identity");
			return res.json();
		},
		enabled: !import.meta.env.DEV,
		retry: 2,
		staleTime: 5 * 60 * 1000,
	});
}

export function UserProvider({ children }: { children: ReactNode }) {
	const query = useUserQuery();
	const user = import.meta.env.DEV ? MOCK_USER : (query.data ?? null);
	return <UserContext value={user}>{children}</UserContext>;
}

export function useUser(): User | null {
	return use(UserContext);
}
