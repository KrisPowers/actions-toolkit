import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare } from "lucide-react";
import { githubApi } from "../api/github";
import StatusBadge from "../components/common/StatusBadge";

export default function IssuesPage() {
  const { repoId } = useParams();
  const [state, setState] = useState<"open" | "closed" | "all">("open");
  const [expanded, setExpanded] = useState<number | null>(null);
  const [comment, setComment] = useState("");
  const qc = useQueryClient();

  const { data: issues, isLoading } = useQuery({
    queryKey: ["issues", repoId, state],
    queryFn: () => githubApi.listIssues(repoId as string, state),
    enabled: !!repoId,
  });

  const addComment = useMutation({
    mutationFn: (number: number) => githubApi.addIssueComment(repoId as string, number, comment),
    onSuccess: () => {
      setComment("");
      qc.invalidateQueries({ queryKey: ["issues", repoId] });
    },
  });

  const updateIssue = useMutation({
    mutationFn: ({ number, targetState }: { number: number; targetState: "open" | "closed" }) =>
      githubApi.updateIssue(repoId as string, number, { state: targetState }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["issues", repoId] }),
  });

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Issues</h1>
        <select
          value={state}
          onChange={(e) => setState(e.target.value as typeof state)}
          className="rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1 text-sm text-neutral-200 outline-none focus:border-accent"
        >
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          <option value="all">All</option>
        </select>
      </div>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(issues ?? [])
          .filter((i: any) => !i.pull_request)
          .map((issue: any) => (
            <div key={issue.number} className="px-4 py-3">
              <div className="flex items-center justify-between">
                <button
                  type="button"
                  onClick={() => setExpanded(expanded === issue.number ? null : issue.number)}
                  className="text-left text-sm text-neutral-200 hover:text-accent"
                >
                  #{issue.number} {issue.title}
                </button>
                <div className="flex items-center gap-2">
                  <StatusBadge status={issue.state} />
                  <button
                    type="button"
                    onClick={() => updateIssue.mutate({ number: issue.number, targetState: issue.state === "open" ? "closed" : "open" })}
                    className="rounded-md border border-neutral-700 px-2 py-0.5 text-xs text-neutral-300 hover:bg-neutral-800"
                  >
                    {issue.state === "open" ? "Close" : "Reopen"}
                  </button>
                </div>
              </div>

              {expanded === issue.number && (
                <div className="mt-3 flex gap-2">
                  <input
                    value={comment}
                    onChange={(e) => setComment(e.target.value)}
                    placeholder="Write a comment…"
                    className="flex-1 rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
                  />
                  <button
                    type="button"
                    disabled={!comment || addComment.isPending}
                    onClick={() => addComment.mutate(issue.number)}
                    className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
                  >
                    <MessageSquare className="h-3.5 w-3.5" strokeWidth={2} />
                    Comment
                  </button>
                </div>
              )}
            </div>
          ))}
        {(issues ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No issues.</div>}
      </div>
    </div>
  );
}
