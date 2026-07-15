import { api } from "./client";
import type { WorkflowModel, WorkflowRow } from "./types";

export const workflowsApi = {
  listForRepo: (repoId: string) => api.get<WorkflowRow[]>(`/repos/${repoId}/workflows`),
  get: (id: string) => api.get<WorkflowRow>(`/workflows/${id}`),
  create: (repoId: string, name: string, source: { yaml_source?: string; workflow_json?: WorkflowModel }) =>
    api.post<WorkflowRow>(`/repos/${repoId}/workflows`, { name, ...source }),
  update: (id: string, source: { yaml_source?: string; workflow_json?: WorkflowModel }) =>
    api.patch<{ workflow: WorkflowRow }>(`/workflows/${id}`, source),
  setEnabled: (id: string, enabled: boolean) => api.patch<void>(`/workflows/${id}/enabled`, { enabled }),
  delete: (id: string) => api.delete<void>(`/workflows/${id}`),
  validate: (source: { yaml_source?: string; workflow_json?: WorkflowModel }) =>
    api.post<{ valid: boolean; error?: string }>("/workflows/validate", source),
  dispatch: (id: string) => api.post(`/workflows/${id}/dispatch`),
};
