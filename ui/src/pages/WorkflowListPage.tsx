import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useCreateWorkflow, useDeleteWorkflow, useWorkflows } from "../hooks/useWorkflows";
import { useDispatchWorkflow } from "../hooks/useWorkflows";
import ConfirmDialog from "../components/common/ConfirmDialog";

const DEFAULT_YAML = `name: New workflow
on:
  push:
    branches: [main]
jobs:
  build:
    steps:
      - name: Say hello
        run: echo "hello from actions-toolkit"
`;

export default function WorkflowListPage() {
  const { repoId } = useParams();
  const { data: workflows } = useWorkflows(repoId);
  const createWorkflow = useCreateWorkflow(repoId as string);
  const deleteWorkflow = useDeleteWorkflow(repoId as string);
  const dispatch = useDispatchWorkflow();
  const [newName, setNewName] = useState("");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  function createNew(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    createWorkflow.mutate({ name: newName.trim(), yaml_source: DEFAULT_YAML.replace("New workflow", newName.trim()) });
    setNewName("");
  }

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Workflows</h1>
        <Link to={`/repos/${repoId}/runs`} className="text-sm text-accent hover:underline">
          View runs →
        </Link>
      </div>

      <form onSubmit={createNew} className="mt-4 flex gap-2">
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          placeholder="New workflow name"
          className="w-64 rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
        />
        <button type="submit" className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-dark">
          Create workflow
        </button>
      </form>

      <div className="mt-6 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(workflows ?? []).map((w) => (
          <div key={w.id} className="flex items-center justify-between px-4 py-3">
            <div>
              <Link to={`/repos/${repoId}/workflows/${w.id}`} className="text-sm font-medium text-neutral-100 hover:text-accent">
                {w.name}
              </Link>
              <div className="mt-0.5 text-xs text-neutral-500">{w.enabled ? "enabled" : "disabled"} · {w.file_path}</div>
            </div>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={() => dispatch.mutate(w.id)}
                className="rounded-md border border-neutral-700 px-2.5 py-1 text-xs text-neutral-200 hover:bg-neutral-800"
              >
                Run now
              </button>
              <button
                type="button"
                onClick={() => setPendingDelete(w.id)}
                className="rounded-md border border-neutral-700 px-2.5 py-1 text-xs text-red-300 hover:bg-red-950/40"
              >
                Delete
              </button>
            </div>
          </div>
        ))}
        {(workflows ?? []).length === 0 && <div className="px-4 py-6 text-sm text-neutral-500">No workflows yet.</div>}
      </div>

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
    </div>
  );
}
