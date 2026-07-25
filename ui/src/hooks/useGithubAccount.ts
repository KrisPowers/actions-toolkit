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

export function useInstallations() {
  return useQuery({ queryKey: ["github", "installations"], queryFn: githubAccountApi.listInstallations });
}

// Re-runs installation discovery against GitHub, so an org the App was just installed on (which
// has no callback into this instance to pick up automatically) shows up without a full reconnect.
export function useRefreshInstallations() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => githubAccountApi.refreshInstallations(),
    onSuccess: (data) => {
      qc.setQueryData(["github", "installations"], data);
      qc.invalidateQueries({ queryKey: ["github", "accessible-repos"] });
    },
  });
}
