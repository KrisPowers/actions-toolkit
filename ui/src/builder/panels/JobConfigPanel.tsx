import type { Job, Step } from "../../api/types";
import ConditionRuleBuilder from "./ConditionRuleBuilder";
import StepConfigPanel from "./StepConfigPanel";

interface Props {
  jobKey: string;
  job: Job;
  onChange: (job: Job) => void;
  onRemove: () => void;
}

function emptyStep(): Step {
  return { name: "New step", run: "", "continue-on-error": false };
}

export default function JobConfigPanel({ jobKey, job, onChange, onRemove }: Props) {
  return (
    <div>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-neutral-100">Job: {jobKey}</h3>
        <button type="button" onClick={onRemove} className="text-xs text-red-400 hover:underline">
          Delete job
        </button>
      </div>

      <label className="mt-4 block text-xs font-medium text-neutral-400">Display name</label>
      <input
        value={job.name ?? ""}
        onChange={(e) => onChange({ ...job, name: e.target.value })}
        className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
      />

      <label className="mt-3 flex items-center gap-2 text-xs font-medium text-neutral-400">
        <input
          type="checkbox"
          checked={job.container != null}
          onChange={(e) =>
            onChange({ ...job, container: e.target.checked ? { image: "", volumes: [] } : undefined })
          }
        />
        Run in a Docker container
      </label>
      <p className="mt-0.5 text-[11px] text-neutral-600">
        {job.container
          ? "run: steps execute inside this container via Docker."
          : "run: steps execute natively via the Bucket sandbox (no container)."}
      </p>
      {job.container && (
        <>
          <label className="mt-2 block text-xs font-medium text-neutral-400">Container image</label>
          <input
            value={job.container.image}
            onChange={(e) => onChange({ ...job, container: { ...job.container!, image: e.target.value } })}
            placeholder="node:20-alpine"
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 font-mono text-sm text-neutral-100 outline-none focus:border-accent"
          />
        </>
      )}

      <div className="mt-3 text-xs font-medium text-neutral-400">
        Depends on ({job.needs.length ? job.needs.join(", ") : "none"})
      </div>
      <p className="mt-0.5 text-[11px] text-neutral-600">Drag a connection from another job's right edge to this job's left edge on the canvas to add a dependency.</p>

      <div className="mt-3">
        <div className="text-xs font-medium text-neutral-400">Run condition</div>
        <ConditionRuleBuilder value={job.if} availableNeeds={job.needs} onChange={(expr) => onChange({ ...job, if: expr })} />
      </div>

      <div className="mt-4">
        <div className="flex items-center justify-between">
          <div className="text-xs font-medium text-neutral-400">Steps</div>
          <button
            type="button"
            onClick={() => onChange({ ...job, steps: [...job.steps, emptyStep()] })}
            className="text-xs text-accent hover:underline"
          >
            + Add step
          </button>
        </div>
        <div className="mt-2 flex flex-col gap-2">
          {job.steps.map((step, i) => (
            <StepConfigPanel
              key={i}
              step={step}
              availableNeeds={job.needs}
              onChange={(s) => onChange({ ...job, steps: job.steps.map((x, j) => (j === i ? s : x)) })}
              onRemove={() => onChange({ ...job, steps: job.steps.filter((_, j) => j !== i) })}
            />
          ))}
        </div>
      </div>

      <div className="mt-4">
        <div className="flex items-center justify-between">
          <div className="text-xs font-medium text-neutral-400">Artifacts produced by this job</div>
          <button
            type="button"
            onClick={() => onChange({ ...job, artifacts: [...job.artifacts, { name: "artifact", path: "/workspace/dist" }] })}
            className="text-xs text-accent hover:underline"
          >
            + Add artifact
          </button>
        </div>
        {job.artifacts.map((a, i) => (
          <div key={i} className="mt-1.5 flex gap-2">
            <input
              value={a.name}
              onChange={(e) =>
                onChange({ ...job, artifacts: job.artifacts.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)) })
              }
              className="w-28 rounded border border-neutral-700 bg-neutral-950 px-2 py-1 text-xs text-neutral-100"
            />
            <input
              value={a.path}
              onChange={(e) =>
                onChange({ ...job, artifacts: job.artifacts.map((x, j) => (j === i ? { ...x, path: e.target.value } : x)) })
              }
              className="flex-1 rounded border border-neutral-700 bg-neutral-950 px-2 py-1 font-mono text-xs text-neutral-100"
            />
            <button
              type="button"
              onClick={() => onChange({ ...job, artifacts: job.artifacts.filter((_, j) => j !== i) })}
              className="text-xs text-red-400 hover:underline"
            >
              ✕
            </button>
          </div>
        ))}
      </div>

      <label className="mt-4 block text-xs font-medium text-neutral-400">Download artifacts from earlier jobs (comma separated)</label>
      <input
        value={job.download_artifacts.join(", ")}
        onChange={(e) => onChange({ ...job, download_artifacts: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })}
        className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-xs text-neutral-100 outline-none focus:border-accent"
      />
    </div>
  );
}
