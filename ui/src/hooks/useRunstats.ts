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

export function useBucketForWebhookEvent(eventId: string | undefined) {
  return useQuery({
    queryKey: ["bucket-for-webhook-event", eventId],
    queryFn: () => runstatsApi.bucketForWebhookEvent(eventId as string),
    enabled: !!eventId,
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
