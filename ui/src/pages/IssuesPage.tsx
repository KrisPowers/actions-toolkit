import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare } from "lucide-react";
import { githubApi } from "../api/github";
import StatusBadge from "../components/common/StatusBadge";
import LabelPill from "../components/common/LabelPill";
import Avatar from "../components/common/Avatar";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import Select from "../components/common/Select";
import { relativeTime } from "../lib/relativeTime";

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
        <Select value={state} onChange={(e) => setState(e.target.value as typeof state)} className="py-1">
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          <option value="all">All</option>
        </Select>
      </div>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(issues ?? [])
          .filter((i) => !i.pull_request)
          .map((issue) => (
            <div key={issue.number} className="px-4 py-3">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <button
                    type="button"
                    onClick={() => setExpanded(expanded === issue.number ? null : issue.number)}
                    className="text-left text-sm text-neutral-200 hover:text-accent"
                  >
                    #{issue.number} {issue.title}
                  </button>
                  <div className="mt-1 flex flex-wrap items-center gap-2">
                    <span className="flex items-center gap-1.5 text-xs text-neutral-500">
                      {issue.user && <Avatar login={issue.user.login} src={issue.user.avatar_url} size={16} />}
                      {issue.state === "open" ? "opened" : "closed"} {relativeTime(issue.created_at)}
                      {issue.user ? ` by ${issue.user.login}` : ""}
                    </span>
                    {issue.labels.map((l) => (
                      <LabelPill key={l.name} name={l.name} color={l.color} />
                    ))}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <StatusBadge status={issue.state} />
                  <Button
                    variant="default"
                    size="sm"
                    onClick={() => updateIssue.mutate({ number: issue.number, targetState: issue.state === "open" ? "closed" : "open" })}
                  >
                    {issue.state === "open" ? "Close" : "Reopen"}
                  </Button>
                </div>
              </div>

              {expanded === issue.number && (
                <div className="mt-3 flex gap-2">
                  <Input
                    value={comment}
                    onChange={(e) => setComment(e.target.value)}
                    placeholder="Write a comment…"
                    className="flex-1"
                  />
                  <Button variant="primary" disabled={!comment || addComment.isPending} onClick={() => addComment.mutate(issue.number)}>
                    <MessageSquare className="h-3.5 w-3.5" strokeWidth={2} />
                    Comment
                  </Button>
                </div>
              )}
            </div>
          ))}
        {(issues ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No issues.</div>}
      </div>
    </div>
  );
}
