import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { reposApi } from "../api/repos";

export function useRepos() {
  return useQuery({ queryKey: ["repos"], queryFn: reposApi.list });
}

export function useRepo(id: string | undefined) {
  return useQuery({ queryKey: ["repos", id], queryFn: () => reposApi.get(id as string), enabled: !!id });
}

export function useCreateRepo() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ owner, name, defaultBranch }: { owner: string; name: string; defaultBranch?: string }) =>
      reposApi.create(owner, name, defaultBranch),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
}

export function useDeleteRepo() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => reposApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
}

export function useTestRepoConnection() {
  return useMutation({ mutationFn: (id: string) => reposApi.testConnection(id) });
}

export function useSyncRepo() {
  return useMutation({ mutationFn: (id: string) => reposApi.sync(id) });
}

export function useRecreateWebhook() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => reposApi.recreateWebhook(id),
    onSuccess: (_data, id) => qc.invalidateQueries({ queryKey: ["repos", id] }),
  });
}
