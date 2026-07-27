import { Hash } from "lucide-react";
import GithubMark from "./GithubMark";
import type { RunRef } from "../../lib/runRef";
import { runRefGithubUrl } from "../../lib/runRef";

export default function RefBadge({ runRef, owner, name }: { runRef: RunRef; owner: string; name: string }) {
  const label = runRef.kind === "pr" ? `${runRef.number}` : runRef.name;
  const title = runRef.kind === "pr" ? `PR #${runRef.number} on GitHub` : `Branch ${runRef.name} on GitHub`;

  return (
    <a
      href={runRefGithubUrl(owner, name, runRef)}
      target="_blank"
      rel="noreferrer"
      title={title}
      onClick={(e) => e.stopPropagation()}
      className="inline-flex max-w-[8rem] shrink-0 items-center gap-1 rounded-full border border-neutral-800 px-2 py-0.5 text-[10px] font-medium text-neutral-500 hover:border-neutral-700 hover:text-neutral-300"
    >
      {runRef.kind === "pr" ? (
        <Hash className="h-2.5 w-2.5 shrink-0" strokeWidth={2.5} />
      ) : (
        <GithubMark className="h-2.5 w-2.5 shrink-0" />
      )}
      <span className="truncate">{label}</span>
    </a>
  );
}
