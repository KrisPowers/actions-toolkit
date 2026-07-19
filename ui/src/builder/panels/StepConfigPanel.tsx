import type { Step } from "../../api/types";
import ConditionRuleBuilder from "./ConditionRuleBuilder";
import Input from "../../components/common/Input";
import Textarea from "../../components/common/Textarea";

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
        <Input
          value={step.name ?? ""}
          onChange={(e) => onChange({ ...step, name: e.target.value })}
          placeholder="Step name"
          className="min-w-0 flex-1 bg-neutral-900 px-2 py-1 text-xs"
        />
        <button type="button" onClick={onRemove} className="text-xs text-[var(--color-status-error)] hover:underline">
          Remove
        </button>
      </div>

      <div className="mt-2 flex gap-2">
        <label className="flex items-center gap-1 text-xs text-neutral-400">
          <input
            type="radio"
            checked={!isContainerAction}
            onChange={() => onChange({ ...step, uses: undefined, run: step.run ?? "" })}
            className="accent-accent"
          />
          Shell command
        </label>
        <label className="flex items-center gap-1 text-xs text-neutral-400">
          <input
            type="radio"
            checked={isContainerAction}
            onChange={() => onChange({ ...step, run: undefined, uses: "docker://" })}
            className="accent-accent"
          />
          Container action
        </label>
      </div>

      {!isContainerAction ? (
        <Textarea
          value={step.run ?? ""}
          onChange={(e) => onChange({ ...step, run: e.target.value })}
          rows={3}
          placeholder="echo hello"
          className="mt-2 w-full bg-neutral-900 px-2 py-1.5 font-mono text-xs"
        />
      ) : (
        <Input
          value={step.uses ?? "docker://"}
          onChange={(e) => onChange({ ...step, uses: e.target.value })}
          placeholder="docker://alpine:3.20"
          className="mt-2 w-full bg-neutral-900 px-2 py-1.5 font-mono text-xs"
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
          className="accent-accent"
        />
        Continue workflow if this step fails
      </label>
    </div>
  );
}
