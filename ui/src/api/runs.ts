import { api } from "./client";
import type { RunLog, RunTree, WorkflowRun } from "./types";

export const runsApi = {
  listForRepo: (repoId: string, limit?: number, offset?: number) => {
    const params = new URLSearchParams();
    if (limit) params.set("limit", String(limit));
    if (offset) params.set("offset", String(offset));
    const qs = params.toString();
    return api.get<WorkflowRun[]>(`/repos/${repoId}/runs${qs ? `?${qs}` : ""}`);
  },
  get: (id: string) => api.get<RunTree>(`/runs/${id}`),
  cancel: (id: string) => api.post<void>(`/runs/${id}/cancel`),
  rerun: (id: string) => api.post<WorkflowRun>(`/runs/${id}/rerun`),
  logs: (id: string, sinceId?: number, stepRunId?: string) => {
    const params = new URLSearchParams();
    if (sinceId) params.set("since_id", String(sinceId));
    if (stepRunId) params.set("step_run_id", stepRunId);
    const qs = params.toString();
    return api.get<RunLog[]>(`/runs/${id}/logs${qs ? `?${qs}` : ""}`);
  },
  logsWsUrl: (id: string, stepRunId?: string) => {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const params = stepRunId ? `?step_run_id=${stepRunId}` : "";
    return `${proto}//${window.location.host}/api/runs/${id}/logs/ws${params}`;
  },
  activityWsUrl: (repoId: string) => {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${proto}//${window.location.host}/api/repos/${repoId}/activity/ws`;
  },
};
