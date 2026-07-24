import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
	api,
	type CreateStudyRequest,
	type Environment,
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
