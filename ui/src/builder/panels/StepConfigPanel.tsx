import type { Step } from "../../api/types";
import ConditionRuleBuilder from "./ConditionRuleBuilder";

interface Props {
  step: Step;
  availableNeeds: string[];
  onChange: (step: Step) => void;
  onRemove: () => void;
}

export default function StepConfigPanel({ step, availableNeeds, onChange, onRemove }: Props) {
  const isContainerAction = step.uses?.startsWith("docker://") ?? false;

  return (
    <div className="rounded-md border border-neutral-800 bg-neutral-950 p-3">
      <div className="flex items-center justify-between gap-2">
        <input
          value={step.name ?? ""}
          onChange={(e) => onChange({ ...step, name: e.target.value })}
          placeholder="Step name"
          className="min-w-0 flex-1 rounded border border-neutral-700 bg-neutral-900 px-2 py-1 text-xs text-neutral-100"
        />
        <button type="button" onClick={onRemove} className="text-xs text-red-400 hover:underline">
          Remove
        </button>
      </div>

      <div className="mt-2 flex gap-2">
        <label className="flex items-center gap-1 text-xs text-neutral-400">
          <input type="radio" checked={!isContainerAction} onChange={() => onChange({ ...step, uses: undefined, run: step.run ?? "" })} />
          Shell command
        </label>
        <label className="flex items-center gap-1 text-xs text-neutral-400">
          <input
            type="radio"
            checked={isContainerAction}
            onChange={() => onChange({ ...step, run: undefined, uses: "docker://" })}
          />
          Container action
        </label>
      </div>

      {!isContainerAction ? (
        <textarea
          value={step.run ?? ""}
          onChange={(e) => onChange({ ...step, run: e.target.value })}
          rows={3}
          placeholder="echo hello"
          className="mt-2 w-full rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 font-mono text-xs text-neutral-100"
        />
      ) : (
        <input
          value={step.uses ?? "docker://"}
          onChange={(e) => onChange({ ...step, uses: e.target.value })}
          placeholder="docker://alpine:3.20"
          className="mt-2 w-full rounded border border-neutral-700 bg-neutral-900 px-2 py-1.5 font-mono text-xs text-neutral-100"
        />
      )}

      <div className="mt-2">
        <div className="text-xs font-medium text-neutral-500">Run condition</div>
        <ConditionRuleBuilder value={step.if} availableNeeds={availableNeeds} onChange={(expr) => onChange({ ...step, if: expr })} />
      </div>

      <label className="mt-2 flex items-center gap-2 text-xs text-neutral-400">
        <input
          type="checkbox"
          checked={step["continue-on-error"]}
          onChange={(e) => onChange({ ...step, "continue-on-error": e.target.checked })}
        />
        Continue workflow if this step fails
      </label>
    </div>
  );
}
