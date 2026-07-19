import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, MessageSquare } from "lucide-react";
import { githubApi } from "../api/github";
import StatusBadge from "../components/common/StatusBadge";
import LabelPill from "../components/common/LabelPill";
import Avatar from "../components/common/Avatar";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import Select from "../components/common/Select";
import { relativeTime } from "../lib/relativeTime";

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

  function prStatus(pr: { state: string; draft: boolean; merged_at: string | null }): string {
    if (pr.merged_at) return "merged";
    if (pr.draft) return "draft";
    return pr.state;
  }

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Pull Requests</h1>
        <Select value={state} onChange={(e) => setState(e.target.value as typeof state)} className="py-1">
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          <option value="all">All</option>
        </Select>
      </div>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(pulls ?? []).map((pr) => (
          <div key={pr.number} className="px-4 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <button
                  type="button"
                  onClick={() => setExpanded(expanded === pr.number ? null : pr.number)}
                  className="text-left text-sm text-neutral-200 hover:text-accent"
                >
                  #{pr.number} {pr.title}
                </button>
                <div className="mt-1 flex flex-wrap items-center gap-2">
                  <span className="flex items-center gap-1.5 text-xs text-neutral-500">
                    {pr.user && <Avatar login={pr.user.login} src={pr.user.avatar_url} size={16} />}
                    opened {relativeTime(pr.created_at)}
                    {pr.user ? ` by ${pr.user.login}` : ""}
                  </span>
                  <span className="inline-flex items-center gap-1 font-mono text-xs text-neutral-500">
                    {pr.head?.ref}
                    <ArrowRight className="h-3 w-3" strokeWidth={2} />
                    {pr.base?.ref}
                  </span>
                  {pr.labels.map((l) => (
                    <LabelPill key={l.name} name={l.name} color={l.color} />
                  ))}
                </div>
              </div>
              <StatusBadge status={prStatus(pr)} />
            </div>

            {expanded === pr.number && (
              <div className="mt-3 flex gap-2">
                <Input value={comment} onChange={(e) => setComment(e.target.value)} placeholder="Write a comment…" className="flex-1" />
                <Button variant="primary" disabled={!comment || addComment.isPending} onClick={() => addComment.mutate(pr.number)}>
                  <MessageSquare className="h-3.5 w-3.5" strokeWidth={2} />
                  Comment
                </Button>
              </div>
            )}
          </div>
        ))}
        {(pulls ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No pull requests.</div>}
      </div>
    </div>
  );
}
