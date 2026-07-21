import { useOutletContext } from "react-router-dom";
import StatusBadge from "../../components/common/StatusBadge";
import Card, { cardClass } from "../../components/common/Card";
import { formatDuration } from "../../lib/duration";
import type { RunDetailContext } from "../RunDetailLayout";

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs text-neutral-500">{label}</div>
      <div className="mt-0.5 text-sm text-neutral-200">{value}</div>
    </div>
  );
}

export default function RunInsightsPanel() {
  const { tree } = useOutletContext<RunDetailContext>();
  const { run } = tree;

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <Card className="grid grid-cols-2 gap-4 p-4 sm:grid-cols-4">
        <Stat label="Duration" value={formatDuration(run.started_at, run.finished_at)} />
        <Stat label="Trigger" value={run.trigger_event} />
        <Stat label="Ref" value={run.ref_name ?? "—"} />
        <Stat label="Commit" value={run.commit_sha ? run.commit_sha.slice(0, 7) : "—"} />
      </Card>

      <div className="mt-4 flex flex-col gap-3">
        {tree.jobs.map((jt) => (
          <div key={jt.job.id} className={cardClass()}>
            <div className="flex items-center justify-between border-b border-neutral-800 px-4 py-2.5">
              <span className="text-sm font-semibold text-neutral-200">{jt.job.name ?? jt.job.job_key}</span>
              <div className="flex items-center gap-3 text-xs text-neutral-500">
                {jt.job.exit_code !== null && <span>exit {jt.job.exit_code}</span>}
                <span>{formatDuration(jt.job.started_at, jt.job.finished_at)}</span>
                <StatusBadge status={jt.job.status} />
              </div>
            </div>
            <div className="divide-y divide-neutral-800">
              {jt.steps.map((step) => (
                <div key={step.id} className="flex items-center justify-between px-4 py-2 text-xs">
                  <span className="text-neutral-300">{step.name ?? `step ${step.step_index + 1}`}</span>
                  <div className="flex items-center gap-3 text-neutral-500">
                    {step.exit_code !== null && <span>exit {step.exit_code}</span>}
                    <span>{formatDuration(step.started_at, step.finished_at)}</span>
                    <StatusBadge status={step.status} />
                  </div>
                </div>
              ))}
              {jt.steps.length === 0 && <div className="px-4 py-2 text-xs text-neutral-600">No steps.</div>}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
