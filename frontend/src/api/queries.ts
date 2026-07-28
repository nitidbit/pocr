import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
	ApiError,
	api,
	type CreateStudyRequest,
	type Environment,
	type StudyRole,
	type StudyStatus,
	type UpdateStudyRequest,
} from "./client";

export const studyKeys = {
	all: ["studies"] as const,
	detail: (id: string) => ["studies", id] as const,
	randomizations: (studyId: string, environment: Environment) =>
		["studies", studyId, "randomizations", environment] as const,
	balance: (studyId: string, environment: Environment) =>
		["studies", studyId, "balance", environment] as const,
	members: (id: string) => ["studies", id, "members"] as const,
};

export function useStudies() {
	return useQuery({
		queryKey: studyKeys.all,
		queryFn: api.studies.list,
	});
}

export function useStudy(id: string) {
	return useQuery({
		queryKey: studyKeys.detail(id),
		queryFn: () => api.studies.get(id),
	});
}

export function useCreateStudy() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (data: CreateStudyRequest) => api.studies.create(data),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.all });
			toast.success("Study created successfully");
		},
		onError: () => {
			toast.error("Failed to create study");
		},
	});
}

export function useUpdateStudy(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (data: UpdateStudyRequest) => api.studies.update(id, data),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.all });
			queryClient.invalidateQueries({ queryKey: studyKeys.detail(id) });
			toast.success("Study updated");
		},
		onError: () => {
			toast.error("Failed to update study");
		},
	});
}

export function useDeleteStudy() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (id: string) => api.studies.delete(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.all });
			toast.success("Study deleted");
		},
		onError: () => {
			toast.error("Failed to delete study");
		},
	});
}

export function useTransitionStudy(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (status: StudyStatus) => api.studies.transition(id, status),
		onSuccess: (study) => {
			queryClient.invalidateQueries({ queryKey: studyKeys.all });
			queryClient.invalidateQueries({ queryKey: studyKeys.detail(id) });
			const STATUS_MESSAGE: Record<string, string> = {
				active: "Study activated",
				paused: "Study paused",
				closed: "Study closed",
			};
			toast.success(STATUS_MESSAGE[study.status] ?? "Study updated");
		},
		onError: () => {
			toast.error("Failed to change study status");
		},
	});
}

export function useSetArmLocator(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ armId, locator }: { armId: string; locator: number }) =>
			api.studies.setArmLocator(id, armId, locator),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.detail(id) });
			queryClient.invalidateQueries({ queryKey: studyKeys.all });
			toast.success("Locator mapped");
		},
		onError: () => {
			toast.error("Failed to map locator");
		},
	});
}

export function useRandomizations(studyId: string, environment: Environment) {
	return useQuery({
		queryKey: studyKeys.randomizations(studyId, environment),
		queryFn: () => api.randomizations.list(studyId, { environment }),
	});
}

export function useBalance(studyId: string, environment: Environment) {
	return useQuery({
		queryKey: studyKeys.balance(studyId, environment),
		queryFn: () => api.balance.get(studyId, environment),
	});
}

export function usePurgeNonProduction(studyId: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: () => api.studies.purgeNonProduction(studyId),
		onSuccess: (data) => {
			queryClient.invalidateQueries({
				queryKey: ["studies", studyId, "randomizations"],
			});
			queryClient.invalidateQueries({
				queryKey: ["studies", studyId, "balance"],
			});
			toast.success(`Purged ${data.recordsRemoved} test randomizations`);
		},
		onError: () => {
			toast.error("Failed to purge test data");
		},
	});
}

export function useSystemStatus() {
	return useQuery({
		queryKey: ["system", "status"],
		queryFn: api.system.status,
		refetchInterval: 30_000,
	});
}

/**
 * Map a membership-mutation failure to a user-facing message.
 *
 * 403 means the caller is not an owner; 409 means the action would leave the
 * study without an owner. Anything else falls back to a generic message.
 */
function memberErrorMessage(error: unknown): string {
	if (error instanceof ApiError) {
		if (error.status === 403) return "Only owners can manage access";
		if (error.status === 409) return "A study must keep at least one owner";
	}
	return "Failed to update access";
}

export function useStudyMembers(id: string) {
	return useQuery({
		queryKey: studyKeys.members(id),
		queryFn: () => api.studies.members.list(id),
	});
}

export function useAddMember(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ email, role }: { email: string; role: StudyRole }) =>
			api.studies.members.add(id, email, role),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.members(id) });
			toast.success("Access granted");
		},
		onError: (error) => {
			toast.error(memberErrorMessage(error));
		},
	});
}

export function useUpdateMemberRole(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ email, role }: { email: string; role: StudyRole }) =>
			api.studies.members.updateRole(id, email, role),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.members(id) });
			toast.success("Role updated");
		},
		onError: (error) => {
			toast.error(memberErrorMessage(error));
		},
	});
}

export function useRemoveMember(id: string) {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (email: string) => api.studies.members.remove(id, email),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: studyKeys.members(id) });
			toast.success("Access revoked");
		},
		onError: (error) => {
			toast.error(memberErrorMessage(error));
		},
	});
}
