import { useQuery } from "@tanstack/react-query";
import { auditLogApi } from "../api/auditLog";

export function useAuditLog(repoId: string | undefined, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ["audit-log", repoId, page],
    queryFn: () => auditLogApi.listForRepo(repoId as string, pageSize, page * pageSize),
    enabled: !!repoId,
  });
}
