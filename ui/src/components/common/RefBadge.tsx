import { GitBranch, Hash } from "lucide-react";
import type { RunRef } from "../../lib/runRef";
import { runRefGithubUrl } from "../../lib/runRef";

export default function RefBadge({ runRef, owner, name }: { runRef: RunRef; owner: string; name: string }) {
  const Icon = runRef.kind === "pr" ? Hash : GitBranch;
  const label = runRef.kind === "pr" ? `${runRef.number}` : runRef.name;
  const title = runRef.kind === "pr" ? `PR #${runRef.number} on GitHub` : `Branch ${runRef.name} on GitHub`;

  return (
    <a
      href={runRefGithubUrl(owner, name, runRef)}
      target="_blank"
      rel="noreferrer"
      title={title}
      onClick={(e) => e.stopPropagation()}
      className="inline-flex max-w-[10rem] items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium hover:brightness-125"
      style={{
        color: "var(--color-status-info)",
        borderColor: "color-mix(in srgb, var(--color-status-info) 30%, transparent)",
        backgroundColor: "color-mix(in srgb, var(--color-status-info) 12%, transparent)",
      }}
    >
      <Icon className="h-3 w-3 shrink-0" strokeWidth={2.5} />
      <span className="truncate">{label}</span>
    </a>
  );
}
