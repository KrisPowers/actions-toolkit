import { useParams } from "react-router-dom";
import { Play } from "lucide-react";
import { useDispatchWorkflow, useUpdateWorkflow, useWorkflow } from "../hooks/useWorkflows";
import WorkflowBuilder from "../builder/WorkflowBuilder";
import Button from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
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
    <div className="flex h-full flex-col">
      <div className="pb-3">
        <PageHeader
          title={workflow.name}
          subtitle={workflow.description ?? undefined}
          backTo={`/repos/${repoId}/workflows`}
          backLabel="Workflows"
          actions={
            <Button variant="default" onClick={() => dispatch.mutate(workflow.id)} disabled={dispatch.isPending}>
              <Play className="h-3.5 w-3.5" strokeWidth={2} />
              {dispatch.isPending ? "Starting…" : "Run now"}
            </Button>
          }
        />
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
