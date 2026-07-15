import { useQuery } from "@tanstack/react-query";
import { artifactsApi } from "../api/artifacts";

export function useArtifacts(runId: string | undefined) {
  return useQuery({
    queryKey: ["artifacts", runId],
    queryFn: () => artifactsApi.listForRun(runId as string),
    enabled: !!runId,
  });
}
