import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { X } from "lucide-react";
import { useCreateWorkflow } from "../../hooks/useWorkflows";
import Modal from "../common/Modal";
import Button from "../common/Button";
import Input from "../common/Input";
import Textarea from "../common/Textarea";

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
    <Modal open onClose={onClose}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-neutral-100">New workflow</h3>
        <Button variant="invisible" size="icon" onClick={onClose} aria-label="Close">
          <X className="h-4 w-4" strokeWidth={2} />
        </Button>
      </div>
      <p className="mt-1 text-xs text-neutral-500">Name it, then continue into the builder to set triggers and steps.</p>

      <form onSubmit={handleNext} className="mt-4 flex flex-col gap-3">
        <div>
          <label className="block text-xs font-medium text-neutral-400">Name</label>
          <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Build and test" className="mt-1 w-full" />
        </div>
        <div>
          <label className="block text-xs font-medium text-neutral-400">Description</label>
          <Textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            placeholder="What does this workflow do? (optional)"
            className="mt-1 w-full resize-none"
          />
        </div>

        {createWorkflow.isError && <p className="text-xs text-[var(--color-status-error)]">{(createWorkflow.error as Error).message}</p>}

        <Button type="submit" variant="primary" disabled={!name.trim() || createWorkflow.isPending} className="mt-1 w-full">
          {createWorkflow.isPending ? "Creating…" : "Next"}
        </Button>
      </form>
    </Modal>
  );
}
