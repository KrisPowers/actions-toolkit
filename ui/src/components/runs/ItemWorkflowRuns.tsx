import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PlayCircle, RotateCcw } from "lucide-react";
import { runsApi } from "../../api/runs";
import type { WorkflowRun } from "../../api/types";
import StatusBadge from "../common/StatusBadge";
import Button from "../common/Button";
import { relativeTime } from "../../lib/relativeTime";

const ACTIVE_STATUSES = new Set(["queued", "running", "pending"]);

// Runs are the app's own execution engine, not a mirror of GitHub's state, so this is scoped
// to rerun/status controls only. Nothing here writes back to the issue/PR/release itself.
export default function ItemWorkflowRuns({
  repoId,
  runs,
  emptyLabel = "this",
}: {
  repoId: string;
  runs: WorkflowRun[];
  emptyLabel?: string;
}) {
  const qc = useQueryClient();
  const invalidate = () => qc.invalidateQueries({ queryKey: ["runs", "repo", repoId] });

  const rerun = useMutation({
    mutationFn: (id: string) => runsApi.rerun(id),
    onSuccess: invalidate,
  });
  const rerunAll = useMutation({
    mutationFn: (ids: string[]) => Promise.all(ids.map((id) => runsApi.rerun(id))),
    onSuccess: invalidate,
  });

  const rerunableIds = runs.filter((r) => !ACTIVE_STATUSES.has(r.status)).map((r) => r.id);

  return (
    <div className="mt-3 border-t border-neutral-800 pt-3">
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-xs font-medium text-neutral-400">
          <PlayCircle className="h-3.5 w-3.5" strokeWidth={2} />
          Workflow runs
        </span>
        {runs.length > 0 && (
          <Button
            variant="default"
            size="sm"
            disabled={rerunableIds.length === 0 || rerunAll.isPending}
            onClick={() => rerunAll.mutate(rerunableIds)}
          >
            <RotateCcw className="h-3.5 w-3.5" strokeWidth={2} />
            {rerunAll.isPending ? "Rerunning…" : "Rerun all"}
          </Button>
        )}
      </div>

      {runs.length === 0 ? (
        <p className="mt-2 text-xs text-neutral-600">No workflow runs for this {emptyLabel} yet.</p>
      ) : (
        <div className="mt-2 flex flex-col gap-1.5">
          {runs.map((run) => (
            <div key={run.id} className="flex items-center justify-between gap-2 rounded border border-neutral-800 bg-neutral-950/40 px-2.5 py-1.5">
              <Link to={`/repos/${repoId}/runs/${run.id}`} className="min-w-0 truncate text-xs text-neutral-300 hover:text-accent">
                {run.trigger_event} · {relativeTime(run.created_at)}
              </Link>
              <div className="flex shrink-0 items-center gap-2">
                <StatusBadge status={run.status} />
                {!ACTIVE_STATUSES.has(run.status) && (
                  <Button
                    variant="invisible"
                    size="icon"
                    title="Rerun this workflow"
                    disabled={rerun.isPending}
                    onClick={() => rerun.mutate(run.id)}
                  >
                    <RotateCcw className="h-3.5 w-3.5" strokeWidth={2} />
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
