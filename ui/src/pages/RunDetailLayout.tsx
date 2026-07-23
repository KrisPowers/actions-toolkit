import { Outlet, useParams } from "react-router-dom";
import { RotateCcw, XCircle } from "lucide-react";
import { useCancelRun, useRerun, useRun } from "../hooks/useRuns";
import StatusBadge from "../components/common/StatusBadge";
import Button from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import RunDetailSidebar from "../components/runs/RunDetailSidebar";
import type { RunTree } from "../api/types";

const ACTIVE_STATUSES = new Set(["queued", "running", "pending"]);

export interface RunDetailContext {
  tree: RunTree;
  runActive: boolean;
}

export default function RunDetailLayout() {
  const { runId } = useParams();
  const { data: tree } = useRun(runId);
  const cancel = useCancelRun();
  const rerun = useRerun();

  if (!tree) return <p className="text-sm text-neutral-500">Loading…</p>;

  const runActive = ACTIVE_STATUSES.has(tree.run.status);

  return (
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader
          title={
            <span className="flex items-center gap-2">
              {tree.run.trigger_event}
              <StatusBadge status={tree.run.status} />
            </span>
          }
          backTo={`/repos/${tree.run.repo_id}/overview`}
          backLabel="Overview"
          actions={
            <>
              {runActive && (
                <Button variant="danger" onClick={() => cancel.mutate(tree.run.id)}>
                  <XCircle className="h-3.5 w-3.5" strokeWidth={2} />
                  Cancel
                </Button>
              )}
              {!runActive && (
                <Button variant="default" onClick={() => rerun.mutate(tree.run.id)}>
                  <RotateCcw className="h-3.5 w-3.5" strokeWidth={2} />
                  Re-run
                </Button>
              )}
            </>
          }
        />
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-6 md:grid-cols-[200px_1fr]">
        <RunDetailSidebar runId={tree.run.id} />
        <div className="flex min-h-0 min-w-0 flex-col">
          <Outlet context={{ tree, runActive } satisfies RunDetailContext} />
        </div>
      </div>
    </div>
  );
}
