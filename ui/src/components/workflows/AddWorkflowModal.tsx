import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { X } from "lucide-react";
import { useCreateWorkflow } from "../../hooks/useWorkflows";

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

export default function AddWorkflowModal({ repoId, onClose }: { repoId: string; onClose: () => void }) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const createWorkflow = useCreateWorkflow(repoId);
  const navigate = useNavigate();

  async function handleNext(e: FormEvent) {
    e.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName) return;
    const workflow = await createWorkflow.mutateAsync({
      name: trimmedName,
      description: description.trim() || undefined,
      yaml_source: DEFAULT_YAML.replace("New workflow", trimmedName),
    });
    navigate(`/repos/${repoId}/workflows/${workflow.id}`);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div
        className="w-full max-w-sm rounded-lg border border-neutral-800 bg-neutral-900 p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-neutral-100">New workflow</h3>
          <button type="button" onClick={onClose} aria-label="Close" className="text-neutral-500 hover:text-neutral-300">
            <X className="h-4 w-4" strokeWidth={2} />
          </button>
        </div>
        <p className="mt-1 text-xs text-neutral-500">Name it, then continue into the builder to set triggers and steps.</p>

        <form onSubmit={handleNext} className="mt-4 flex flex-col gap-3">
          <div>
            <label className="block text-xs font-medium text-neutral-400">Name</label>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Build and test"
              className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-neutral-400">Description</label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="What does this workflow do? (optional)"
              className="mt-1 w-full resize-none rounded-md border border-neutral-700 bg-neutral-950 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-accent"
            />
          </div>

          {createWorkflow.isError && (
            <p className="text-xs text-[var(--color-status-error)]">{(createWorkflow.error as Error).message}</p>
          )}

          <button
            type="submit"
            disabled={!name.trim() || createWorkflow.isPending}
            className="mt-1 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-60"
          >
            {createWorkflow.isPending ? "Creating…" : "Next"}
          </button>
        </form>
      </div>
    </div>
  );
}
