import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { runsApi } from "../api/runs";
import type { WorkflowRun } from "../api/types";

export function useRuns(repoId: string | undefined, limit?: number) {
  return useQuery({
    queryKey: ["runs", "repo", repoId, limit],
    queryFn: () => runsApi.listForRepo(repoId as string, limit),
    enabled: !!repoId,
    refetchInterval: 5000,
  });
}

/**
 * Live push for new runs: prepends a run to every cached `useRuns` list for this repo the moment
 * it's created, instead of waiting on the next 5s poll. Mirrors `useLiveLogs`/`useLiveStats`.
 */
export function useLiveRunActivity(repoId: string | undefined) {
  const qc = useQueryClient();

  useEffect(() => {
    if (!repoId) return;
    const ws = new WebSocket(runsApi.activityWsUrl(repoId));
    ws.onmessage = (event) => {
      try {
        const run = JSON.parse(event.data) as WorkflowRun;
        qc.setQueriesData<WorkflowRun[]>({ queryKey: ["runs", "repo", repoId] }, (old) => {
          if (!old) return old;
          if (old.some((r) => r.id === run.id)) return old;
          return [run, ...old];
        });
      } catch {
        // ignore malformed frames
      }
    };
    return () => ws.close();
  }, [repoId, qc]);
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
