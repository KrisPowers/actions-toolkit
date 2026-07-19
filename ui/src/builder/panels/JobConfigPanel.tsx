import { Plus, X } from "lucide-react";
import type { Job, Step } from "../../api/types";
import { useRuntimeStatus } from "../../hooks/useSettings";
import ConditionRuleBuilder from "./ConditionRuleBuilder";
import StepConfigPanel from "./StepConfigPanel";
import Input from "../../components/common/Input";
import Button from "../../components/common/Button";
import { fieldClass } from "../../lib/fieldClass";

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
  const { data: runtimeStatus } = useRuntimeStatus();
  const dockerAvailable = runtimeStatus?.docker_available ?? true; // assume available until the first check lands, to avoid a flash of "disabled"
  const usingDocker = job.container != null;

  return (
    <div>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-neutral-100">Job: {jobKey}</h3>
        <button type="button" onClick={onRemove} className="text-xs text-[var(--color-status-error)] hover:underline">
          Delete job
        </button>
      </div>

      <label className="mt-4 block text-xs font-medium text-neutral-400">Display name</label>
      <Input value={job.name ?? ""} onChange={(e) => onChange({ ...job, name: e.target.value })} className="mt-1 w-full" />

      <div className="mt-3 text-xs font-medium text-neutral-400">Sandbox</div>
      <div className="mt-1 flex w-fit gap-1 rounded-md border border-neutral-700 p-0.5">
        <button
          type="button"
          onClick={() => onChange({ ...job, container: undefined })}
          className={`rounded px-2.5 py-1 text-xs font-medium ${!usingDocker ? "bg-accent text-white" : "text-neutral-400 hover:text-neutral-200"}`}
        >
          Bucket (native)
        </button>
        <button
          type="button"
          disabled={!dockerAvailable}
          title={!dockerAvailable ? "Docker isn't reachable on this host right now." : undefined}
          onClick={() => onChange({ ...job, container: job.container ?? { image: "", volumes: [] } })}
          className={`rounded px-2.5 py-1 text-xs font-medium ${usingDocker ? "bg-accent text-white" : "text-neutral-400 hover:text-neutral-200"} ${
            !dockerAvailable ? "cursor-not-allowed opacity-40" : ""
          }`}
        >
          Docker
        </button>
      </div>
      <p className="mt-1 text-[11px] text-neutral-600">
        {usingDocker
          ? "run: steps execute inside this container via Docker."
          : "run: steps execute natively via the Bucket sandbox (no container)."}
      </p>
      {!dockerAvailable && (
        <p className="mt-0.5 text-[11px] text-[var(--color-status-warning)]">
          Docker isn't running or reachable right now, so it can't be selected for new jobs.
          {usingDocker && " This job is still configured for Docker and will fail to run until Docker is available."}
        </p>
      )}
      {job.container && (
        <>
          <label className="mt-2 block text-xs font-medium text-neutral-400">Container image</label>
          <Input
            value={job.container.image}
            onChange={(e) => onChange({ ...job, container: { ...job.container!, image: e.target.value } })}
            placeholder="node:20-alpine"
            className="mt-1 w-full font-mono"
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
            className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
          >
            <Plus className="h-3 w-3" strokeWidth={2} />
            Add step
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
            className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
          >
            <Plus className="h-3 w-3" strokeWidth={2} />
            Add artifact
          </button>
        </div>
        {job.artifacts.map((a, i) => (
          <div key={i} className="mt-1.5 flex items-center gap-2">
            <input
              value={a.name}
              onChange={(e) =>
                onChange({ ...job, artifacts: job.artifacts.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)) })
              }
              className={fieldClass("w-28 px-2 py-1 text-xs")}
            />
            <input
              value={a.path}
              onChange={(e) =>
                onChange({ ...job, artifacts: job.artifacts.map((x, j) => (j === i ? { ...x, path: e.target.value } : x)) })
              }
              className={fieldClass("flex-1 px-2 py-1 font-mono text-xs")}
            />
            <Button
              variant="invisible"
              size="icon"
              onClick={() => onChange({ ...job, artifacts: job.artifacts.filter((_, j) => j !== i) })}
              aria-label={`Remove artifact ${a.name}`}
              className="hover:text-[var(--color-status-error)]"
            >
              <X className="h-3.5 w-3.5" strokeWidth={2} />
            </Button>
          </div>
        ))}
      </div>

      <label className="mt-4 block text-xs font-medium text-neutral-400">Download artifacts from earlier jobs (comma separated)</label>
      <Input
        value={job.download_artifacts.join(", ")}
        onChange={(e) => onChange({ ...job, download_artifacts: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })}
        className="mt-1 w-full text-xs"
      />
    </div>
  );
}
