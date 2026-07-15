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
    mutationFn: ({ owner, name, pat, defaultBranch }: { owner: string; name: string; pat: string; defaultBranch?: string }) =>
      reposApi.create(owner, name, pat, defaultBranch),
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

export function useUpdateRepoPat(repoId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (pat: string) => reposApi.updatePat(repoId, pat),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos", repoId] }),
  });
}
