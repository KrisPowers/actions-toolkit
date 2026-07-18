import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { githubAccountApi } from "../api/githubAccount";

export function useGithubTokenStatus() {
  return useQuery({ queryKey: ["github", "token-status"], queryFn: githubAccountApi.status });
}

export function useDeleteGithubToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => githubAccountApi.deleteToken(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["github", "token-status"] });
      qc.invalidateQueries({ queryKey: ["auth", "status"] });
    },
  });
}

export function useAccessibleRepos(enabled: boolean) {
  return useQuery({
    queryKey: ["github", "accessible-repos"],
    queryFn: githubAccountApi.accessibleRepos,
    enabled,
  });
}
