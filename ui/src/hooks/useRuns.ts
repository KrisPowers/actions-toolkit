import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { runsApi } from "../api/runs";

export function useRuns(repoId: string | undefined, limit?: number) {
  return useQuery({
    queryKey: ["runs", "repo", repoId, limit],
    queryFn: () => runsApi.listForRepo(repoId as string, limit),
    enabled: !!repoId,
    refetchInterval: 5000,
  });
}

export function useRunsForEvent(eventId: string | undefined) {
  return useQuery({
    queryKey: ["runs", "webhook-event", eventId],
    queryFn: () => runsApi.listForWebhookEvent(eventId as string),
    enabled: !!eventId,
  });
}

export function useRun(id: string | undefined) {
  return useQuery({
    queryKey: ["runs", id],
    queryFn: () => runsApi.get(id as string),
    enabled: !!id,
    refetchInterval: (query) => {
      const status = query.state.data?.run.status;
      return status === "queued" || status === "running" ? 2000 : false;
    },
  });
}

export function useCancelRun() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => runsApi.cancel(id),
    onSuccess: (_data, id) => qc.invalidateQueries({ queryKey: ["runs", id] }),
  });
}

export function useRerun() {
  return useMutation({ mutationFn: (id: string) => runsApi.rerun(id) });
}
