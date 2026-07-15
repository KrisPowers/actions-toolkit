import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { workflowsApi } from "../api/workflows";
import type { WorkflowModel } from "../api/types";

export function useWorkflows(repoId: string | undefined) {
  return useQuery({
    queryKey: ["workflows", "repo", repoId],
    queryFn: () => workflowsApi.listForRepo(repoId as string),
    enabled: !!repoId,
  });
}

export function useWorkflow(id: string | undefined) {
  return useQuery({ queryKey: ["workflows", id], queryFn: () => workflowsApi.get(id as string), enabled: !!id });
}

export function useCreateWorkflow(repoId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, yaml_source, workflow_json }: { name: string; yaml_source?: string; workflow_json?: WorkflowModel }) =>
      workflowsApi.create(repoId, name, { yaml_source, workflow_json }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workflows", "repo", repoId] }),
  });
}

export function useUpdateWorkflow(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (source: { yaml_source?: string; workflow_json?: WorkflowModel }) => workflowsApi.update(id, source),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workflows", id] }),
  });
}

export function useDeleteWorkflow(repoId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => workflowsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workflows", "repo", repoId] }),
  });
}

export function useDispatchWorkflow() {
  return useMutation({ mutationFn: (id: string) => workflowsApi.dispatch(id) });
}
