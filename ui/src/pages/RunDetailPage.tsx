import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ArrowLeft, Package, RotateCcw, XCircle } from "lucide-react";
import { useCancelRun, useRerun, useRun } from "../hooks/useRuns";
import { useLiveLogs } from "../hooks/useLiveLogs";
import StatusBadge from "../components/common/StatusBadge";
import LogViewer from "../components/logs/LogViewer";

const ACTIVE_STATUSES = new Set(["queued", "running", "pending"]);

export default function RunDetailPage() {
  const { runId } = useParams();
  const { data: tree } = useRun(runId);
  const cancel = useCancelRun();
  const rerun = useRerun();
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);

  const runActive = tree ? ACTIVE_STATUSES.has(tree.run.status) : false;
  const lines = useLiveLogs(runId, runActive);
  const filteredLines = selectedStepId ? lines.filter((l) => l.step_run_id === selectedStepId) : lines;

  if (!tree) return <p className="text-sm text-neutral-500">Loading…</p>;

  return (
    <div className="flex h-[calc(100vh-6.5rem)] flex-col">
      <div className="flex items-center justify-between pb-3">
        <div>
          <Link to={`/repos/${tree.run.repo_id}/runs`} className="inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300">
            <ArrowLeft className="h-3 w-3" strokeWidth={2} />
            Runs
          </Link>
          <div className="mt-0.5 flex items-center gap-2">
            <h1 className="text-lg font-semibold text-neutral-100">{tree.run.trigger_event}</h1>
            <StatusBadge status={tree.run.status} />
          </div>
        </div>
        <div className="flex gap-2">
          <Link
            to={`/runs/${tree.run.id}/artifacts`}
            className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
          >
            <Package className="h-3.5 w-3.5" strokeWidth={2} />
            Artifacts
          </Link>
          {runActive && (
            <button
              type="button"
              onClick={() => cancel.mutate(tree.run.id)}
              className="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-status-error)]/40 px-3 py-1.5 text-sm text-[var(--color-status-error)] hover:bg-[var(--color-status-error)]/10"
            >
              <XCircle className="h-3.5 w-3.5" strokeWidth={2} />
              Cancel
            </button>
          )}
          {!runActive && (
            <button
              type="button"
              onClick={() => rerun.mutate(tree.run.id)}
              className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            >
              <RotateCcw className="h-3.5 w-3.5" strokeWidth={2} />
              Re-run
            </button>
          )}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 gap-3">
        <div className="w-64 shrink-0 overflow-y-auto rounded-lg border border-neutral-800 bg-neutral-900 p-2">
          <button
            type="button"
            onClick={() => setSelectedStepId(null)}
            className={`mb-2 w-full rounded px-2 py-1 text-left text-xs ${!selectedStepId ? "bg-accent/15 text-accent" : "text-neutral-400 hover:bg-neutral-800"}`}
          >
            All output
          </button>
          {tree.jobs.map((jt) => (
            <div key={jt.job.id} className="mb-2">
              <div className="flex items-center justify-between px-2 py-1">
                <span className="text-xs font-semibold text-neutral-300">{jt.job.name ?? jt.job.job_key}</span>
                <StatusBadge status={jt.job.status} />
              </div>
              {jt.steps.map((step) => (
                <button
                  key={step.id}
                  type="button"
                  onClick={() => setSelectedStepId(step.id)}
                  className={`flex w-full items-center justify-between rounded px-3 py-1 text-left text-xs ${
                    selectedStepId === step.id ? "bg-accent/15 text-accent" : "text-neutral-400 hover:bg-neutral-800"
                  }`}
                >
                  <span className="truncate">{step.name ?? `step ${step.step_index + 1}`}</span>
                  <StatusBadge status={step.status} />
                </button>
              ))}
            </div>
          ))}
        </div>

        <div className="min-h-0 min-w-0 flex-1">
          <LogViewer lines={filteredLines} />
        </div>
      </div>
    </div>
  );
}
