import { api } from "./client";
import type { Artifact } from "./types";

export const artifactsApi = {
  listForRun: (runId: string) => api.get<Artifact[]>(`/runs/${runId}/artifacts`),
  downloadUrl: (id: string) => `/api/artifacts/${id}/download`,
};
