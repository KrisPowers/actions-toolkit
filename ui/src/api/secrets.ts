import { api } from "./client";
import type { Secret } from "./types";

export const secretsApi = {
  listForRepo: (repoId: string) => api.get<Secret[]>(`/repos/${repoId}/secrets`),
  create: (repoId: string, name: string, value: string) => api.post<Secret>(`/repos/${repoId}/secrets`, { name, value }),
  delete: (repoId: string, id: string) => api.delete<void>(`/repos/${repoId}/secrets/${id}`),
};
