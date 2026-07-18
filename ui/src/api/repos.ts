import { api } from "./client";
import type { RepoPublic, WebhookEvent } from "./types";

export interface CreateRepoResponse extends RepoPublic {
  webhook_secret: string;
}

export const reposApi = {
  list: () => api.get<RepoPublic[]>("/repos"),
  get: (id: string) => api.get<RepoPublic>(`/repos/${id}`),
  create: (owner: string, name: string, default_branch?: string) =>
    api.post<CreateRepoResponse>("/repos", { owner, name, default_branch }),
  delete: (id: string) => api.delete<void>(`/repos/${id}`),
  testConnection: (id: string) => api.post<{ ok: boolean; message: string }>(`/repos/${id}/test-connection`),
  webhookEvents: (id: string) => api.get<WebhookEvent[]>(`/repos/${id}/webhook-events`),
};
