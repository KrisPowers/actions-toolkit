import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ArrowRight, Play, Plus, Trash2 } from "lucide-react";
import { useDeleteWorkflow, useWorkflows } from "../hooks/useWorkflows";
import { useDispatchWorkflow } from "../hooks/useWorkflows";
import ConfirmDialog from "../components/common/ConfirmDialog";
import AddWorkflowModal from "../components/workflows/AddWorkflowModal";
import GithubWorkflowsSection from "../components/workflows/GithubWorkflowsSection";
import Button from "../components/common/Button";

export default function WorkflowListPage() {
  const { repoId } = useParams();
  const { data: workflows } = useWorkflows(repoId);
  const deleteWorkflow = useDeleteWorkflow(repoId as string);
  const dispatch = useDispatchWorkflow();
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Workflows</h1>
        <div className="flex items-center gap-3">
          <Link to={`/repos/${repoId}/runs`} className="inline-flex items-center gap-1 text-sm text-accent hover:underline">
            View runs
            <ArrowRight className="h-3.5 w-3.5" strokeWidth={2} />
          </Link>
          <Button variant="primary" onClick={() => setShowAddModal(true)}>
            <Plus className="h-3.5 w-3.5" strokeWidth={2} />
            Add workflow
          </Button>
        </div>
      </div>

      <div className="mt-6 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(workflows ?? []).map((w) => (
          <div key={w.id} className="flex items-center justify-between px-4 py-3">
            <div>
              <Link to={`/repos/${repoId}/workflows/${w.id}`} className="text-sm font-medium text-neutral-100 hover:text-accent">
                {w.name}
              </Link>
              <div className="mt-0.5 text-xs text-neutral-500">
                {w.enabled ? "enabled" : "disabled"} · <span className="font-mono">{w.file_path}</span>
              </div>
              {w.description && <div className="mt-1 max-w-md text-xs text-neutral-400">{w.description}</div>}
            </div>
            <div className="flex items-center gap-3">
              <Button variant="default" size="sm" onClick={() => dispatch.mutate(w.id)}>
                <Play className="h-3 w-3" strokeWidth={2} />
                Run now
              </Button>
              <Button variant="danger" size="sm" onClick={() => setPendingDelete(w.id)} aria-label={`Delete ${w.name}`}>
                <Trash2 className="h-3 w-3" strokeWidth={2} />
                Delete
              </Button>
            </div>
          </div>
        ))}
        {(workflows ?? []).length === 0 && <div className="px-4 py-6 text-sm text-neutral-500">No workflows yet.</div>}
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
