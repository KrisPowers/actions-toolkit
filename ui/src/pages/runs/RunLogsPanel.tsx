import { useState } from "react";
import { useOutletContext, useParams } from "react-router-dom";
import { useLiveLogs } from "../../hooks/useLiveLogs";
import StatusBadge from "../../components/common/StatusBadge";
import LogViewer from "../../components/logs/LogViewer";
import Card from "../../components/common/Card";
import { formatDuration } from "../../lib/duration";
import type { RunDetailContext } from "../RunDetailLayout";

export default function RunLogsPanel() {
  const { runId } = useParams();
  const { tree, runActive } = useOutletContext<RunDetailContext>();
  const [activeStepId, setActiveStepId] = useState<string | null>(null);

  const lines = useLiveLogs(runId, runActive);

  return (
    <div className="flex min-h-0 flex-1 gap-3">
      <Card className="w-64 shrink-0 overflow-y-auto p-2">
        <button
          type="button"
          onClick={() => setActiveStepId(null)}
          className={`mb-2 w-full rounded border-l-2 px-2 py-1.5 text-left text-xs ${
            !activeStepId ? "border-accent bg-accent/10 font-medium text-neutral-100" : "border-transparent text-neutral-400 hover:bg-neutral-800"
          }`}
        >
          All output
        </button>
        {tree.jobs.map((jt) => (
          <div key={jt.job.id} className="mb-2">
            <div className="flex items-center gap-2 px-2 py-1.5">
              <StatusBadge status={jt.job.status} iconOnly />
              <span className="truncate text-xs font-semibold text-neutral-300">{jt.job.name ?? jt.job.job_key}</span>
            </div>
            {jt.steps.map((step) => (
              <button
                key={step.id}
                type="button"
                onClick={() => setActiveStepId(step.id)}
                className={`flex w-full items-center gap-2 rounded border-l-2 py-1.5 pl-4 pr-2 text-left text-xs ${
                  activeStepId === step.id
                    ? "border-accent bg-accent/10 font-medium text-neutral-100"
                    : "border-transparent text-neutral-400 hover:bg-neutral-800"
                }`}
              >
                <StatusBadge status={step.status} iconOnly />
                <span className="min-w-0 flex-1 truncate">{step.name ?? `step ${step.step_index + 1}`}</span>
                <span className="shrink-0 tabular-nums text-neutral-600">{formatDuration(step.started_at, step.finished_at)}</span>
              </button>
            ))}
          </div>
        ))}
      </Card>

      <div className="min-h-0 min-w-0 flex-1">
        <LogViewer lines={lines} activeStepId={activeStepId} />
      </div>
    </div>
  );
}
