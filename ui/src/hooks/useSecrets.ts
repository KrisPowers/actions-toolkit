import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { secretsApi } from "../api/secrets";

export function useSecrets(repoId: string | undefined) {
  return useQuery({
    queryKey: ["repos", repoId, "secrets"],
    queryFn: () => secretsApi.listForRepo(repoId as string),
    enabled: !!repoId,
  });
}

export function useCreateSecret(repoId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, value }: { name: string; value: string }) => secretsApi.create(repoId, name, value),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos", repoId, "secrets"] }),
  });
}

export function useDeleteSecret(repoId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => secretsApi.delete(repoId, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos", repoId, "secrets"] }),
  });
}
