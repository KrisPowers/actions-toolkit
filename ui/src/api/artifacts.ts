import { api } from "./client";
import type { Artifact, ArtifactWithContext } from "./types";

export const artifactsApi = {
  listForRun: (runId: string) => api.get<Artifact[]>(`/runs/${runId}/artifacts`),
  listForRepo: (repoId: string) => api.get<ArtifactWithContext[]>(`/repos/${repoId}/artifacts`),
  downloadUrl: (id: string) => `/api/artifacts/${id}/download`,
};
