import type { TriggerConfig } from "../../api/types";
import Input from "../../components/common/Input";

interface Props {
  on: TriggerConfig;
  onChange: (on: TriggerConfig) => void;
}

function listInput(label: string, value: string[], onChange: (v: string[]) => void, placeholder: string) {
  return (
    <div className="mt-3">
      <label className="block text-xs font-medium text-neutral-400">{label}</label>
      <Input
        value={value.join(", ")}
        onChange={(e) => onChange(e.target.value.split(",").map((s) => s.trim()).filter(Boolean))}
        placeholder={placeholder}
        className="mt-1 w-full font-mono"
      />
    </div>
  );
}

export default function TriggerConfigPanel({ on, onChange }: Props) {
  return (
    <div>
      <h3 className="text-sm font-semibold text-neutral-100">Trigger</h3>

      <label className="mt-4 flex items-center gap-2 text-sm text-neutral-200">
        <input
          type="checkbox"
          className="accent-accent"
          checked={!!on.push}
          onChange={(e) => onChange({ ...on, push: e.target.checked ? { branches: [], tags: [], paths: [] } : null })}
        />
        On push
      </label>
      {on.push && (
        <div className="ml-6">
          {listInput("Branches (glob)", on.push.branches, (branches) => onChange({ ...on, push: { ...on.push!, branches } }), "main, release/*")}
          {listInput("Tags (glob)", on.push.tags, (tags) => onChange({ ...on, push: { ...on.push!, tags } }), "v*")}
          {listInput("Paths (glob)", on.push.paths, (paths) => onChange({ ...on, push: { ...on.push!, paths } }), "src/**")}
        </div>
      )}

      <label className="mt-4 flex items-center gap-2 text-sm text-neutral-200">
        <input
          type="checkbox"
          className="accent-accent"
          checked={!!on.pull_request}
          onChange={(e) => onChange({ ...on, pull_request: e.target.checked ? { types: ["opened", "synchronize"], branches: [] } : null })}
        />
        On pull request (includes new commits in a PR via "synchronize")
      </label>
      {on.pull_request && (
        <div className="ml-6">
          {listInput(
            "Branches (base, glob)",
            on.pull_request.branches,
            (branches) => onChange({ ...on, pull_request: { ...on.pull_request!, branches } }),
            "main",
          )}
        </div>
      )}

      <label className="mt-4 flex items-center gap-2 text-sm text-neutral-200">
        <input
          type="checkbox"
          className="accent-accent"
          checked={!!on.release}
          onChange={(e) => onChange({ ...on, release: e.target.checked ? { types: ["published"] } : null })}
        />
        On release
      </label>

      <label className="mt-4 flex items-center gap-2 text-sm text-neutral-200">
        <input
          type="checkbox"
          className="accent-accent"
          checked={!!on.workflow_dispatch}
          onChange={(e) => onChange({ ...on, workflow_dispatch: e.target.checked ? {} : null })}
        />
        Allow manual "Run now"
      </label>
    </div>
  );
}
