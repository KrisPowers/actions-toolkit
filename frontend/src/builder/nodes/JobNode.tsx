import { Handle, Position } from "reactflow";
import type { Job } from "../../api/types";

export default function JobNode({ data }: { data: { jobKey: string; job: Job } }) {
  const { jobKey, job } = data;

  return (
    <div className="w-60 rounded-lg border border-neutral-700 bg-neutral-900 px-3 py-2.5 shadow hover:border-accent/60">
      <Handle type="target" position={Position.Left} className="!bg-neutral-500" />
      <div className="text-sm font-semibold text-neutral-100">{job.name || jobKey}</div>
      <div className="mt-1 truncate text-xs text-neutral-500">{job.container.image || "no image set"}</div>
      <div className="mt-2 flex items-center gap-2 text-[11px] text-neutral-500">
        <span>{job.steps.length} step{job.steps.length === 1 ? "" : "s"}</span>
        {job.artifacts.length > 0 && <span>· {job.artifacts.length} artifact{job.artifacts.length === 1 ? "" : "s"}</span>}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-accent" />
    </div>
  );
}
