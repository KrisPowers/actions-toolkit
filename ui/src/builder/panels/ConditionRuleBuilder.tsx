import { useState } from "react";
import Input from "../../components/common/Input";
import Select from "../../components/common/Select";

interface Props {
  value: string | null | undefined;
  availableNeeds: string[];
  onChange: (expr: string | null) => void;
}

const FIELDS = ["github.event_name", ...(["always()", "success()", "failure()"] as const)];

/**
 * Generates the same small `if:` expression strings the backend's expression evaluator
 * understands (workflow::expr) so conditions built here also work if hand-edited later in
 * the YAML code editor.
 */
export default function ConditionRuleBuilder({ value, availableNeeds, onChange }: Props) {
  const [raw, setRaw] = useState(false);

  const needFields = availableNeeds.map((n) => `needs.${n}.result`);
  const options = [...FIELDS, ...needFields];

  if (raw) {
    return (
      <div className="mt-2">
        <Input
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value || null)}
          placeholder="${{ github.event_name == 'push' }}"
          className="w-full font-mono text-xs"
        />
        <button type="button" onClick={() => setRaw(false)} className="mt-1 text-xs text-accent hover:underline">
          Use rule builder
        </button>
      </div>
    );
  }

  const isFunctionCall = value === "always()" || value === "success()" || value === "failure()";
  const match = !isFunctionCall && value ? value.match(/^\s*\$?\{?\{?\s*([\w.]+)\s*==\s*'([^']*)'\s*\}?\}?\s*$/) : null;

  return (
    <div className="mt-2 flex flex-wrap items-center gap-2">
      <Select
        value={isFunctionCall ? value! : match ? "field" : "none"}
        onChange={(e) => {
          const v = e.target.value;
          if (v === "none") onChange(null);
          else if (v === "field") onChange(`\${{ ${options[0]} == '' }}`);
          else onChange(v);
        }}
        className="px-2 py-1 text-xs"
      >
        <option value="none">Always run on success (default)</option>
        <option value="always()">always()</option>
        <option value="failure()">failure()</option>
        <option value="field">field equals value…</option>
      </Select>

      {(match || (!isFunctionCall && value)) && (
        <>
          <Select
            value={match?.[1] ?? options[0]}
            onChange={(e) => onChange(`\${{ ${e.target.value} == '${match?.[2] ?? ""}' }}`)}
            className="px-2 py-1 text-xs"
          >
            {options.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </Select>
          <span className="text-xs text-neutral-500">==</span>
          <Input
            value={match?.[2] ?? ""}
            onChange={(e) => onChange(`\${{ ${match?.[1] ?? options[0]} == '${e.target.value}' }}`)}
            className="w-28 px-2 py-1 text-xs"
          />
        </>
      )}

      <button type="button" onClick={() => setRaw(true)} className="text-xs text-accent hover:underline">
        Edit as text
      </button>
    </div>
  );
}
