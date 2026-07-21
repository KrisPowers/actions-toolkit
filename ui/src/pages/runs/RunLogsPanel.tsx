import { useState } from "react";
import { useOutletContext, useParams } from "react-router-dom";
import { useLiveLogs } from "../../hooks/useLiveLogs";
import StatusBadge from "../../components/common/StatusBadge";
import LogViewer from "../../components/logs/LogViewer";
import Card from "../../components/common/Card";
import type { RunDetailContext } from "../RunDetailLayout";

export default function RunLogsPanel() {
  const { runId } = useParams();
  const { tree, runActive } = useOutletContext<RunDetailContext>();
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);

  const lines = useLiveLogs(runId, runActive);
  const filteredLines = selectedStepId ? lines.filter((l) => l.step_run_id === selectedStepId) : lines;

  return (
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
  );
}
