import { useQuery } from "@tanstack/react-query";
import { analyticsApi } from "../api/analytics";

export function useAnalyticsSummary(repoId: string | undefined) {
  return useQuery({
    queryKey: ["analytics", "summary", repoId],
    queryFn: () => analyticsApi.summary(repoId as string),
    enabled: !!repoId,
  });
}

export function useDurationTrend(repoId: string | undefined, days = 30) {
  return useQuery({
    queryKey: ["analytics", "trend", repoId, days],
    queryFn: () => analyticsApi.durationTrend(repoId as string, days),
    enabled: !!repoId,
  });
}

export function useStatusBreakdown(repoId: string | undefined) {
  return useQuery({
    queryKey: ["analytics", "status", repoId],
    queryFn: () => analyticsApi.statusBreakdown(repoId as string),
    enabled: !!repoId,
  });
}
