import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { LucideIcon } from "lucide-react";
import {
  AlertCircle,
  Boxes,
  Clock,
  GitCommitHorizontal,
  GitPullRequest,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Tag,
  Trash2,
  Workflow,
  X,
  Zap,
} from "lucide-react";
import { useDeleteWorkflow, useDispatchWorkflow, useWorkflows } from "../hooks/useWorkflows";
import { useRepo } from "../hooks/useRepos";
import { useBucketsForRepo } from "../hooks/useRunstats";
import ConfirmDialog from "../components/common/ConfirmDialog";
import AddWorkflowModal from "../components/workflows/AddWorkflowModal";
import GithubWorkflowsSection from "../components/workflows/GithubWorkflowsSection";
import Button, { buttonClass } from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import StatusBadge from "../components/common/StatusBadge";
import { cardClass, listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";
import WebhookUnreachableBanner from "../components/common/WebhookUnreachableBanner";
import type { BucketSummary, WorkflowRow } from "../api/types";

const TRIGGER_ICONS: Record<string, LucideIcon> = {
  push: GitCommitHorizontal,
  pull_request: GitPullRequest,
  release: Tag,
  issues: AlertCircle,
  manual: Play,
  rerun: RotateCcw,
  schedule: Clock,
};

function triggerIcon(kind: string): LucideIcon {
  return TRIGGER_ICONS[kind] ?? Zap;
}

function triggerLabel(kind: string): string {
  return kind.replace(/_/g, " ");
}

function WorkflowCatalogRow({
  workflow,
  selected,
  onSelect,
  onDispatch,
  onDelete,
}: {
  workflow: WorkflowRow;
  selected: boolean;
  onSelect: () => void;
  onDispatch: () => void;
  onDelete: () => void;
}) {
  return (
    <div className={`flex items-center gap-1 pr-2 ${selected ? "bg-accent/10" : ""}`}>
      <button
        type="button"
        onClick={onSelect}
        className={`min-w-0 flex-1 px-4 py-2.5 text-left text-sm ${selected ? "font-medium text-neutral-100" : "text-neutral-300 hover:text-neutral-100"}`}
      >
        <div className="flex items-center gap-1.5">
          <Workflow className="h-3.5 w-3.5 shrink-0 text-neutral-500" strokeWidth={2} />
          <span className="truncate">{workflow.name}</span>
          {!workflow.enabled && <span className="shrink-0 text-[10px] uppercase text-neutral-600">disabled</span>}
        </div>
      </button>
      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100 hover:opacity-100">
        <button
          type="button"
          onClick={onDispatch}
          title="Run now"
          className="flex h-6 w-6 items-center justify-center rounded text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
        >
          <Play className="h-3 w-3" strokeWidth={2} />
        </button>
        <Link
          to={`/repos/${workflow.repo_id}/workflows/${workflow.id}`}
          title="Edit"
          className="flex h-6 w-6 items-center justify-center rounded text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
        >
          <Pencil className="h-3 w-3" strokeWidth={2} />
        </Link>
        <button
          type="button"
          onClick={onDelete}
          title="Delete"
          aria-label={`Delete ${workflow.name}`}
          className="flex h-6 w-6 items-center justify-center rounded text-neutral-500 hover:bg-[var(--color-status-error)] hover:text-white"
        >
          <Trash2 className="h-3 w-3" strokeWidth={2} />
        </button>
      </div>
    </div>
  );
}

function BucketRow({ summary }: { summary: BucketSummary }) {
  const { bucket, shell_count } = summary;
  const Icon = triggerIcon(bucket.trigger_kind);
  return (
    <Link to={`/buckets/${bucket.id}`} className="flex items-center justify-between gap-3 px-4 py-3 hover:bg-neutral-800/50">
      <div className="flex min-w-0 items-center gap-2.5">
        <Icon className="h-4 w-4 shrink-0 text-neutral-500" strokeWidth={2} />
        <div className="min-w-0">
          <div className="truncate text-sm capitalize text-neutral-200">{triggerLabel(bucket.trigger_kind)}</div>
          <div className="mt-0.5 text-xs text-neutral-500">
            {new Date(bucket.created_at).toLocaleString()} · {shell_count} workflow{shell_count === 1 ? "" : "s"}
          </div>
        </div>
      </div>
      <StatusBadge status={bucket.status} />
    </Link>
  );
}

export default function OverviewPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: workflows } = useWorkflows(repoId);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string | null>(null);
  const { data: buckets, isLoading: bucketsLoading } = useBucketsForRepo(repoId, selectedWorkflowId ?? undefined);

  const deleteWorkflow = useDeleteWorkflow(repoId as string);
  const dispatch = useDispatchWorkflow();
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  const selectedWorkflow = (workflows ?? []).find((w) => w.id === selectedWorkflowId);

  return (
    <div>
      <PageHeader
        title="Overview"
        subtitle="Workflows on the left, what triggered them on the right."
        actions={
          <Button variant="primary" onClick={() => setShowAddModal(true)}>
            <Plus className="h-3.5 w-3.5" strokeWidth={2} />
            Add workflow
          </Button>
        }
      />

      {repo && !repo.webhook_connected && (
        <div className="mt-4">
          <WebhookUnreachableBanner />
        </div>
      )}

      <div className="mt-6 grid grid-cols-1 gap-6 md:grid-cols-[280px_1fr]">
        <section className="min-w-0">
          <h2 className="mb-2 text-sm font-semibold text-neutral-200">Workflows</h2>
          <div className={cardClass("overflow-hidden")}>
            <button
              type="button"
              onClick={() => setSelectedWorkflowId(null)}
              className={`w-full border-b border-neutral-800 px-4 py-2.5 text-left text-sm ${
                selectedWorkflowId === null ? "bg-accent/10 font-medium text-neutral-100" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              All workflows
            </button>
            {(workflows ?? []).map((w, i) => (
              <div key={w.id} className={`group ${i < (workflows?.length ?? 0) - 1 ? "border-b border-neutral-800" : ""}`}>
                <WorkflowCatalogRow
                  workflow={w}
                  selected={selectedWorkflowId === w.id}
                  onSelect={() => setSelectedWorkflowId(w.id)}
                  onDispatch={() => dispatch.mutate(w.id)}
                  onDelete={() => setPendingDelete(w.id)}
                />
              </div>
            ))}
            {(workflows ?? []).length === 0 && <EmptyState icon={Workflow} message="No workflows yet." />}
          </div>
        </section>

        <section className="min-w-0">
          <div className="mb-2 flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold text-neutral-200">Recent activity</h2>
            {selectedWorkflow && (
              <button
                type="button"
                onClick={() => setSelectedWorkflowId(null)}
                className={buttonClass("invisible", "sm")}
              >
                <X className="h-3 w-3" strokeWidth={2} />
                {selectedWorkflow.name}
              </button>
            )}
          </div>
          <div className={listCardClass()}>
            {bucketsLoading && <p className="px-4 py-3 text-sm text-neutral-500">Loading…</p>}
            {(buckets ?? []).map((summary) => (
              <BucketRow key={summary.bucket.id} summary={summary} />
            ))}
            {(buckets ?? []).length === 0 && !bucketsLoading && <EmptyState icon={Boxes} message="No triggering events yet." />}
          </div>
        </section>
      </div>

      {repoId && <GithubWorkflowsSection repoId={repoId} />}

      <ConfirmDialog
        open={!!pendingDelete}
        title="Delete workflow"
        message="This deletes the workflow definition. Past runs are kept for history."
        confirmLabel="Delete"
        danger
        onCancel={() => setPendingDelete(null)}
        onConfirm={() => {
          if (pendingDelete) deleteWorkflow.mutate(pendingDelete);
          setPendingDelete(null);
        }}
      />

      {showAddModal && repoId && <AddWorkflowModal repoId={repoId} onClose={() => setShowAddModal(false)} />}
    </div>
  );
}
