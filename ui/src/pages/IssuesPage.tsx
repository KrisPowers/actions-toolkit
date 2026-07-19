import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleDot, MessageSquare } from "lucide-react";
import { githubApi } from "../api/github";
import { useRuns } from "../hooks/useRuns";
import StatusBadge from "../components/common/StatusBadge";
import LabelPill from "../components/common/LabelPill";
import Avatar from "../components/common/Avatar";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import PageHeader from "../components/common/PageHeader";
import { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";
import { TabList, TabButton } from "../components/common/Tabs";
import ItemWorkflowRuns from "../components/runs/ItemWorkflowRuns";
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

  const { data: runs } = useRuns(repoId, 200);
  const runsForIssue = (number: number) => (runs ?? []).filter((r) => r.ref_name === `refs/issues/${number}`);

  const addComment = useMutation({
    mutationFn: (number: number) => githubApi.addIssueComment(repoId as string, number, comment),
    onSuccess: () => {
      setComment("");
      qc.invalidateQueries({ queryKey: ["issues", repoId] });
    },
  });

  return (
    <div>
      <PageHeader title="Issues" />

      <TabList className="mt-4">
        <TabButton active={state === "open"} onClick={() => setState("open")}>
          Open
        </TabButton>
        <TabButton active={state === "closed"} onClick={() => setState("closed")}>
          Closed
        </TabButton>
        <TabButton active={state === "all"} onClick={() => setState("all")}>
          All
        </TabButton>
      </TabList>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass("mt-4")}>
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
                    onClick={() => setExpanded(expanded === issue.number ? null : issue.number)}
                  >
                    Workflows{runsForIssue(issue.number).length > 0 ? ` (${runsForIssue(issue.number).length})` : ""}
                  </Button>
                </div>
              </div>

              {expanded === issue.number && (
                <>
                  <ItemWorkflowRuns repoId={repoId as string} runs={runsForIssue(issue.number)} emptyLabel="issue" />
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
                </>
              )}
            </div>
          ))}
        {(issues ?? []).length === 0 && !isLoading && <EmptyState icon={CircleDot} message="No issues." />}
      </div>
    </div>
  );
}
