import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	RouterProvider,
} from "@tanstack/react-router";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError, api, type StudyMember } from "#/api/client";
import { useUser } from "#/lib/auth";
import { Route as AccessRoute } from "./access";

vi.mock("#/api/client", async () => {
	const actual =
		await vi.importActual<typeof import("#/api/client")>("#/api/client");
	return {
		...actual,
		api: {
			studies: {
				members: {
					list: vi.fn(),
					add: vi.fn(),
					updateRole: vi.fn(),
					remove: vi.fn(),
				},
			},
		},
	};
});

vi.mock("#/lib/auth", () => ({
	useUser: vi.fn(),
}));

vi.mock("sonner", () => ({
	toast: { success: vi.fn(), error: vi.fn() },
}));

// biome-ignore lint/style/noNonNullAssertion: component is always defined on file routes
const StudyAccessPage = AccessRoute.options.component!;

const OWNER: StudyMember = {
	userId: "owner@ucsf.edu",
	role: "owner",
	joinedAt: "2026-01-01T00:00:00Z",
};
const MEMBER: StudyMember = {
	userId: "member@ucsf.edu",
	role: "member",
	joinedAt: "2026-02-01T00:00:00Z",
};

function mockMe(email: string | null) {
	vi.mocked(useUser).mockReturnValue(
		email
			? { email, name: "Test", givenName: "Test", picture: "", groups: [] }
			: null,
	);
}

function renderAccess() {
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
	});

	const rootRoute = createRootRoute();
	const accessRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/studies/$studyId/access",
		component: StudyAccessPage,
	});
	const catchAllRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "$",
	});

	const router = createRouter({
		routeTree: rootRoute.addChildren([accessRoute, catchAllRoute]),
		history: createMemoryHistory({
			initialEntries: ["/studies/study-1/access"],
		}),
	});

	return render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("StudyAccessPage", () => {
	beforeEach(() => {
		vi.mocked(api.studies.members.list).mockResolvedValue([OWNER, MEMBER]);
		vi.mocked(api.studies.members.add).mockResolvedValue(MEMBER);
		vi.mocked(api.studies.members.updateRole).mockResolvedValue(MEMBER);
		vi.mocked(api.studies.members.remove).mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it("lists every member returned by the API", async () => {
		mockMe(OWNER.userId);
		renderAccess();

		expect(await screen.findByText(OWNER.userId)).toBeInTheDocument();
		expect(screen.getByText(MEMBER.userId)).toBeInTheDocument();
	});

	it("enables the add form inputs for an owner", async () => {
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(OWNER.userId);
		expect(screen.getByLabelText(/email/i)).toBeEnabled();
		expect(screen.getByLabelText(/role for new member/i)).toBeEnabled();
		expect(
			screen.queryByText(/only owners can manage access/i),
		).not.toBeInTheDocument();
	});

	it("treats a mixed-case identity as owner of a lowercase-stored row", async () => {
		mockMe("Owner@UCSF.edu");
		renderAccess();

		await screen.findByText(OWNER.userId);
		expect(screen.getByLabelText(/email/i)).toBeEnabled();
		expect(
			screen.queryByText(/only owners can manage access/i),
		).not.toBeInTheDocument();
	});

	it("disables controls and shows a note for a non-owner", async () => {
		mockMe(MEMBER.userId);
		renderAccess();

		expect(
			await screen.findByText(/only owners can manage access/i),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /add/i })).toBeDisabled();
		expect(screen.getByLabelText(/email/i)).toBeDisabled();
	});

	it("calls add with the entered email and selected role", async () => {
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(OWNER.userId);
		fireEvent.change(screen.getByLabelText(/email/i), {
			target: { value: "new@ucsf.edu" },
		});
		fireEvent.change(screen.getByLabelText(/role for new member/i), {
			target: { value: "owner" },
		});
		fireEvent.click(screen.getByRole("button", { name: /add/i }));

		await waitFor(() =>
			expect(api.studies.members.add).toHaveBeenCalledWith(
				"study-1",
				"new@ucsf.edu",
				"owner",
			),
		);
	});

	it("calls updateRole when an owner changes a member's role", async () => {
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(MEMBER.userId);
		fireEvent.change(screen.getByLabelText(`Role for ${MEMBER.userId}`), {
			target: { value: "owner" },
		});

		await waitFor(() =>
			expect(api.studies.members.updateRole).toHaveBeenCalledWith(
				"study-1",
				MEMBER.userId,
				"owner",
			),
		);
	});

	it("calls remove after confirming member removal", async () => {
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(MEMBER.userId);
		fireEvent.click(
			screen.getByRole("button", { name: `Remove ${MEMBER.userId}` }),
		);
		fireEvent.click(screen.getByRole("button", { name: /confirm/i }));

		await waitFor(() =>
			expect(api.studies.members.remove).toHaveBeenCalledWith(
				"study-1",
				MEMBER.userId,
			),
		);
	});

	it("shows an owner-only error toast when add returns 403", async () => {
		const { toast } = await import("sonner");
		vi.mocked(api.studies.members.add).mockRejectedValue(
			new ApiError(403, null),
		);
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(OWNER.userId);
		fireEvent.change(screen.getByLabelText(/email/i), {
			target: { value: "new@ucsf.edu" },
		});
		fireEvent.click(screen.getByRole("button", { name: /add/i }));

		await waitFor(() =>
			expect(toast.error).toHaveBeenCalledWith("Only owners can manage access"),
		);
	});

	it("shows a last-owner error toast when a mutation returns 409", async () => {
		const { toast } = await import("sonner");
		vi.mocked(api.studies.members.updateRole).mockRejectedValue(
			new ApiError(409, null),
		);
		mockMe(OWNER.userId);
		renderAccess();

		await screen.findByText(OWNER.userId);
		fireEvent.change(screen.getByLabelText(`Role for ${OWNER.userId}`), {
			target: { value: "member" },
		});

		await waitFor(() =>
			expect(toast.error).toHaveBeenCalledWith(
				"A study must keep at least one owner",
			),
		);
	});
});
