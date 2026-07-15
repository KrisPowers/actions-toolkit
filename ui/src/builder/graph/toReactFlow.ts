import type { Edge, Node } from "reactflow";
import type { WorkflowModel } from "../../api/types";

const COLUMN_WIDTH = 280;
const ROW_HEIGHT = 160;

/**
 * Positions aren't part of the backend workflow model (only `needs:` dependencies are), so
 * layout is recomputed on every load: jobs are placed in columns by dependency depth
 * (topological level), left to right, so `needs:` edges always point rightward.
 */
export function toReactFlow(workflow: WorkflowModel): { nodes: Node[]; edges: Edge[] } {
  const jobKeys = Object.keys(workflow.jobs);
  const depth = new Map<string, number>();

  function depthOf(key: string, seen: Set<string>): number {
    if (depth.has(key)) return depth.get(key)!;
    if (seen.has(key)) return 0; // cycle guard; validation happens server-side
    seen.add(key);
    const job = workflow.jobs[key];
    const d = job.needs.length === 0 ? 0 : 1 + Math.max(...job.needs.map((n) => depthOf(n, seen)));
    depth.set(key, d);
    return d;
  }
  jobKeys.forEach((k) => depthOf(k, new Set()));

  const columnCounts = new Map<number, number>();
  const nodes: Node[] = [
    {
      id: "__trigger__",
      type: "trigger",
      position: { x: 0, y: 0 },
      data: { on: workflow.on },
      draggable: false,
    },
  ];

  for (const key of jobKeys) {
    const d = depth.get(key) ?? 0;
    const row = columnCounts.get(d) ?? 0;
    columnCounts.set(d, row + 1);
    nodes.push({
      id: key,
      type: "job",
      position: { x: (d + 1) * COLUMN_WIDTH, y: row * ROW_HEIGHT },
      data: { jobKey: key, job: workflow.jobs[key] },
    });
  }

  const edges: Edge[] = [];
  for (const key of jobKeys) {
    const job = workflow.jobs[key];
    for (const need of job.needs) {
      edges.push({ id: `${need}->${key}`, source: need, target: key });
    }
  }

  return { nodes, edges };
}
