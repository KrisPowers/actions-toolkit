import type { TriggerConfig } from "../../api/types";

export default function TriggerNode({ data }: { data: { on: TriggerConfig } }) {
  const events: string[] = [];
  if (data.on.push) events.push("push");
  if (data.on.pull_request) events.push("pull_request");
  if (data.on.release) events.push("release");
  if (data.on.issues) events.push("issues");
  if (data.on.workflow_dispatch) events.push("manual");
  if (data.on.schedule?.length) events.push("schedule");

  return (
    <div className="w-56 rounded-lg border border-accent/40 bg-neutral-900 px-3 py-2.5 shadow">
      <div className="text-xs font-semibold uppercase tracking-wide text-accent">Trigger</div>
      <div className="mt-1 text-sm text-neutral-200">{events.length ? events.join(", ") : "no events configured"}</div>
      <div className="mt-1 text-[11px] text-neutral-500">Every job below runs when this trigger matches, unless it declares needs:.</div>
    </div>
  );
}
