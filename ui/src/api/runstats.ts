import { api } from "./client";
import type { Bucket, BucketTopology, RunStatsSummary, RunTopology } from "./types";

export const runstatsApi = {
  topologyForRun: (runId: string) => api.get<RunTopology>(`/runs/${runId}/topology`),
  statsForRun: (runId: string) => api.get<RunStatsSummary>(`/runs/${runId}/stats`),
  statsWsUrl: (runId: string) => {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${proto}//${window.location.host}/api/runs/${runId}/stats/ws`;
  },
  bucketForWebhookEvent: (eventId: string) => api.get<Bucket | null>(`/webhook-events/${eventId}/bucket`),
  topologyForBucket: (bucketId: string) => api.get<BucketTopology>(`/buckets/${bucketId}/topology`),
};
