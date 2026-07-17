import { Link, useParams } from "react-router-dom";
import { ArrowLeft, Play } from "lucide-react";
import { useDispatchWorkflow, useUpdateWorkflow, useWorkflow } from "../hooks/useWorkflows";
import WorkflowBuilder from "../builder/WorkflowBuilder";
import type { WorkflowModel } from "../api/types";

export default function WorkflowEditorPage() {
  const { repoId, workflowId } = useParams();
  const { data: workflow, isLoading } = useWorkflow(workflowId);
  const update = useUpdateWorkflow(workflowId as string);
  const dispatch = useDispatchWorkflow();

  if (isLoading || !workflow) {
    return <p className="text-sm text-neutral-500">Loading…</p>;
  }

  async function handleSave(source: { yaml_source?: string; workflow_json?: WorkflowModel }) {
    const res = await update.mutateAsync(source);
    return { yaml_source: res.workflow.yaml_source };
  }

  return (
    <div className="flex h-[calc(100vh-6.5rem)] flex-col">
      <div className="flex items-center justify-between pb-3">
        <div>
          <Link to={`/repos/${repoId}/workflows`} className="inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300">
            <ArrowLeft className="h-3 w-3" strokeWidth={2} />
            Workflows
          </Link>
          <h1 className="mt-0.5 text-lg font-semibold text-neutral-100">{workflow.name}</h1>
          {workflow.description && <p className="mt-0.5 text-xs text-neutral-500">{workflow.description}</p>}
        </div>
        <button
          type="button"
          onClick={() => dispatch.mutate(workflow.id)}
          disabled={dispatch.isPending}
          className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
        >
          <Play className="h-3.5 w-3.5" strokeWidth={2} />
          {dispatch.isPending ? "Starting…" : "Run now"}
        </button>
      </div>

      <div className="min-h-0 flex-1">
        <WorkflowBuilder
          name={workflow.name}
          initialYaml={workflow.yaml_source}
          onSave={handleSave}
          saving={update.isPending}
          saveError={update.isError ? (update.error as Error).message : null}
        />
      </div>
    </div>
  );
}
