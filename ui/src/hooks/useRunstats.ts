import { useQuery } from "@tanstack/react-query";
import { runstatsApi } from "../api/runstats";

export function useRunTopology(runId: string | undefined, active: boolean) {
  return useQuery({
    queryKey: ["run-topology", runId],
    queryFn: () => runstatsApi.topologyForRun(runId as string),
    enabled: !!runId,
    refetchInterval: active ? 3000 : false,
  });
}

export function useRunStatsSummary(runId: string | undefined, active: boolean) {
  return useQuery({
    queryKey: ["run-stats-summary", runId],
    queryFn: () => runstatsApi.statsForRun(runId as string),
    enabled: !!runId,
    refetchInterval: active ? 3000 : false,
  });
}

export function useBucketTopology(bucketId: string | undefined) {
  return useQuery({
    queryKey: ["bucket-topology", bucketId],
    queryFn: () => runstatsApi.topologyForBucket(bucketId as string),
    enabled: !!bucketId,
    refetchInterval: 5000,
  });
}

export function useBucketsForRepo(repoId: string | undefined, workflowId?: string) {
  return useQuery({
    queryKey: ["buckets", "repo", repoId, workflowId],
    queryFn: () => runstatsApi.listForRepo(repoId as string, workflowId),
    enabled: !!repoId,
    refetchInterval: 5000,
  });
}
