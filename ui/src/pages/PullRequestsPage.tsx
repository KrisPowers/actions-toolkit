import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, MessageSquare } from "lucide-react";
import { githubApi } from "../api/github";
import StatusBadge from "../components/common/StatusBadge";

export default function PullRequestsPage() {
  const { repoId } = useParams();
  const [state, setState] = useState<"open" | "closed" | "all">("open");
  const [expanded, setExpanded] = useState<number | null>(null);
  const [comment, setComment] = useState("");
  const qc = useQueryClient();

  const { data: pulls, isLoading } = useQuery({
    queryKey: ["pulls", repoId, state],
    queryFn: () => githubApi.listPullRequests(repoId as string, state),
    enabled: !!repoId,
  });

  const addComment = useMutation({
    mutationFn: (number: number) => githubApi.addPrComment(repoId as string, number, comment),
    onSuccess: () => {
      setComment("");
      qc.invalidateQueries({ queryKey: ["pulls", repoId] });
    },
  });

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Pull Requests</h1>
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
        {(pulls ?? []).map((pr: any) => (
          <div key={pr.number} className="px-4 py-3">
            <div className="flex items-center justify-between">
              <button
                type="button"
                onClick={() => setExpanded(expanded === pr.number ? null : pr.number)}
                className="text-left text-sm text-neutral-200 hover:text-accent"
              >
                #{pr.number} {pr.title}
              </button>
              <div className="flex items-center gap-2">
                <span className="inline-flex items-center gap-1 text-xs text-neutral-500">
                  {pr.head?.ref}
                  <ArrowRight className="h-3 w-3" strokeWidth={2} />
                  {pr.base?.ref}
                </span>
                <StatusBadge status={pr.merged_at ? "merged" : pr.state} />
              </div>
            </div>

            {expanded === pr.number && (
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
                  onClick={() => addComment.mutate(pr.number)}
                  className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
                >
                  <MessageSquare className="h-3.5 w-3.5" strokeWidth={2} />
                  Comment
                </button>
              </div>
            )}
          </div>
        ))}
        {(pulls ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No pull requests.</div>}
      </div>
    </div>
  );
}
