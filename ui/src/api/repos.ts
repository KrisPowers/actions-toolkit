import { api } from "./client";
import type { RepoPublic } from "./types";

export type CreateRepoResponse = RepoPublic;

export const reposApi = {
  list: () => api.get<RepoPublic[]>("/repos"),
  get: (id: string) => api.get<RepoPublic>(`/repos/${id}`),
  create: (owner: string, name: string, default_branch?: string) =>
    api.post<CreateRepoResponse>("/repos", { owner, name, default_branch }),
  delete: (id: string) => api.delete<void>(`/repos/${id}`),
  testConnection: (id: string) => api.post<{ ok: boolean; message: string }>(`/repos/${id}/test-connection`),
  sync: (id: string) => api.post<{ dispatched: boolean }>(`/repos/${id}/sync`),
  recreateWebhook: (id: string) => api.post<RepoPublic>(`/repos/${id}/webhooks/recreate`),
};
