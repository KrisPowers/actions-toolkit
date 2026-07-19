import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Package, RotateCcw, XCircle } from "lucide-react";
import { useCancelRun, useRerun, useRun } from "../hooks/useRuns";
import { useLiveLogs } from "../hooks/useLiveLogs";
import StatusBadge from "../components/common/StatusBadge";
import LogViewer from "../components/logs/LogViewer";
import Button, { buttonClass } from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import Card from "../components/common/Card";

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
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader
          title={
            <span className="flex items-center gap-2">
              {tree.run.trigger_event}
              <StatusBadge status={tree.run.status} />
            </span>
          }
          backTo={`/repos/${tree.run.repo_id}/runs`}
          backLabel="Runs"
          actions={
            <>
              <Link to={`/runs/${tree.run.id}/artifacts`} className={buttonClass("default")}>
                <Package className="h-3.5 w-3.5" strokeWidth={2} />
                Artifacts
              </Link>
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

      <div className="flex min-h-0 flex-1 gap-3">
        <Card className="w-64 shrink-0 overflow-y-auto p-2">
          <button
            type="button"
            onClick={() => setSelectedStepId(null)}
            className={`mb-2 w-full rounded border-l-2 px-2 py-1 text-left text-xs ${
              !selectedStepId ? "border-accent bg-accent/10 font-medium text-neutral-100" : "border-transparent text-neutral-400 hover:bg-neutral-800"
            }`}
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
                  className={`flex w-full items-center justify-between rounded border-l-2 px-3 py-1 text-left text-xs ${
                    selectedStepId === step.id
                      ? "border-accent bg-accent/10 font-medium text-neutral-100"
                      : "border-transparent text-neutral-400 hover:bg-neutral-800"
                  }`}
                >
                  <span className="truncate">{step.name ?? `step ${step.step_index + 1}`}</span>
                  <StatusBadge status={step.status} />
                </button>
              ))}
            </div>
          ))}
        </Card>

        <div className="min-h-0 min-w-0 flex-1">
          <LogViewer lines={filteredLines} />
        </div>
      </div>
    </div>
  );
}
