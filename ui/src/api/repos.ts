import { api } from "./client";
import type { RepoPublic } from "./types";

export interface CreateRepoResponse extends RepoPublic {
  webhook_secret: string;
}

export const reposApi = {
  list: () => api.get<RepoPublic[]>("/repos"),
  get: (id: string) => api.get<RepoPublic>(`/repos/${id}`),
  create: (owner: string, name: string, pat: string, default_branch?: string) =>
    api.post<CreateRepoResponse>("/repos", { owner, name, pat, default_branch }),
  updatePat: (id: string, pat: string) => api.patch<RepoPublic>(`/repos/${id}/pat`, { pat }),
  delete: (id: string) => api.delete<void>(`/repos/${id}`),
  testConnection: (id: string) => api.post<{ ok: boolean; message: string }>(`/repos/${id}/test-connection`),
};
