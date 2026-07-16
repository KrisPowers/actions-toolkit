import { api } from "./client";

export interface AnalyticsSummary {
  total_runs: number;
  succeeded: number;
  failed: number;
  cancelled: number;
  success_rate: number;
  avg_duration_seconds: number | null;
}

export interface DurationTrendPoint {
  day: string;
  avg_duration_seconds: number | null;
  run_count: number;
}

export interface StatusCount {
  status: string;
  count: number;
}

export const analyticsApi = {
  summary: (repoId: string) => api.get<AnalyticsSummary>(`/repos/${repoId}/analytics/summary`),
  durationTrend: (repoId: string, days = 30) =>
    api.get<DurationTrendPoint[]>(`/repos/${repoId}/analytics/duration-trend?days=${days}`),
  statusBreakdown: (repoId: string) => api.get<StatusCount[]>(`/repos/${repoId}/analytics/status-breakdown`),
};
