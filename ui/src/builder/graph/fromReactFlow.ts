import type { Edge, Node } from "reactflow";
import type { Job, TriggerConfig, WorkflowModel } from "../../api/types";

export function fromReactFlow(name: string, nodes: Node[], edges: Edge[]): WorkflowModel {
  const triggerNode = nodes.find((n) => n.id === "__trigger__");
  const on: TriggerConfig = (triggerNode?.data.on as TriggerConfig) ?? {};

  const jobs: Record<string, Job> = {};
  for (const node of nodes) {
    if (node.type !== "job") continue;
    const jobKey = node.data.jobKey as string;
    const job = node.data.job as Job;
    const needs = edges.filter((e) => e.target === node.id).map((e) => e.source);
    jobs[jobKey] = { ...job, needs };
  }

  return { name, on, jobs };
}
