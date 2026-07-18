import { useQuery } from "@tanstack/react-query";
import { artifactsApi } from "../api/artifacts";

export function useArtifacts(runId: string | undefined) {
  return useQuery({
    queryKey: ["artifacts", runId],
    queryFn: () => artifactsApi.listForRun(runId as string),
    enabled: !!runId,
  });
}

export function useRepoArtifacts(repoId: string | undefined) {
  return useQuery({
    queryKey: ["artifacts", "repo", repoId],
    queryFn: () => artifactsApi.listForRepo(repoId as string),
    enabled: !!repoId,
  });
}
