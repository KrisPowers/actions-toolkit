import { api } from "./client";
import type { AuditLogEntry } from "./types";

export const auditLogApi = {
  listForRepo: (repoId: string, limit = 50, offset = 0) =>
    api.get<AuditLogEntry[]>(`/repos/${repoId}/audit-log?limit=${limit}&offset=${offset}`),
};
