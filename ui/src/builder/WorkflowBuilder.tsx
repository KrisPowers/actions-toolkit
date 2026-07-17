import { useCallback, useMemo, useState } from "react";
import * as yaml from "js-yaml";
import ReactFlow, { Background, Controls, addEdge, applyEdgeChanges, applyNodeChanges } from "reactflow";
import type { Connection, Edge, EdgeChange, Node, NodeChange } from "reactflow";
import "reactflow/dist/style.css";
import { Plus, Save } from "lucide-react";
import { useTheme } from "../theme/ThemeProvider";

import type { Job, WorkflowModel } from "../api/types";
import YamlCodeEditor from "./YamlCodeEditor";
import TriggerNode from "./nodes/TriggerNode";
import JobNode from "./nodes/JobNode";
import TriggerConfigPanel from "./panels/TriggerConfigPanel";
import JobConfigPanel from "./panels/JobConfigPanel";
import { toReactFlow } from "./graph/toReactFlow";
import { fromReactFlow } from "./graph/fromReactFlow";

const NODE_TYPES = { trigger: TriggerNode, job: JobNode };

interface Props {
  name: string;
  initialYaml: string;
  onSave: (source: { yaml_source?: string; workflow_json?: WorkflowModel }) => Promise<{ yaml_source: string }>;
  saving: boolean;
  saveError?: string | null;
}

function emptyModel(name: string): WorkflowModel {
  return { name, on: { workflow_dispatch: {} }, jobs: {} };
}

function parseYaml(source: string, fallbackName: string): { model: WorkflowModel | null; error: string | null } {
  try {
    const parsed = yaml.load(source) as WorkflowModel;
    if (!parsed || typeof parsed !== "object") return { model: emptyModel(fallbackName), error: null };
    return { model: { ...parsed, jobs: parsed.jobs ?? {}, on: parsed.on ?? {} }, error: null };
  } catch (e) {
    return { model: null, error: (e as Error).message };
  }
}

export default function WorkflowBuilder({ name, initialYaml, onSave, saving, saveError }: Props) {
  const { resolvedTheme } = useTheme();
  const [mode, setMode] = useState<"visual" | "code">("code");
  const [yamlText, setYamlText] = useState(initialYaml);
  const [parseError, setParseError] = useState<string | null>(null);
  const [model, setModel] = useState<WorkflowModel>(() => parseYaml(initialYaml, name).model ?? emptyModel(name));

  const graph = useMemo(() => toReactFlow(model), [model]);
  const [nodes, setNodes] = useState<Node[]>(graph.nodes);
  const [edges, setEdges] = useState<Edge[]>(graph.edges);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const syncModelFromGraph = useCallback(
    (nextNodes: Node[], nextEdges: Edge[]) => {
      setModel(fromReactFlow(name, nextNodes, nextEdges));
    },
    [name],
  );

  function switchToVisual() {
    const { model: parsed, error } = parseYaml(yamlText, name);
    if (!parsed) {
      setParseError(error);
      return;
    }
    setParseError(null);
    setModel(parsed);
    const g = toReactFlow(parsed);
    setNodes(g.nodes);
    setEdges(g.edges);
    setMode("visual");
  }

  function switchToCode() {
    setYamlText(yaml.dump(model, { lineWidth: 100 }));
    setMode("code");
  }

  function onNodesChange(changes: NodeChange[]) {
    setNodes((nds) => {
      const next = applyNodeChanges(changes, nds);
      if (changes.some((c) => c.type === "remove")) syncModelFromGraph(next, edges);
      return next;
    });
  }

  function onEdgesChange(changes: EdgeChange[]) {
    setEdges((eds) => {
      const next = applyEdgeChanges(changes, eds);
      syncModelFromGraph(nodes, next);
      return next;
    });
  }

  function onConnect(connection: Connection) {
    setEdges((eds) => {
      const next = addEdge(connection, eds);
      syncModelFromGraph(nodes, next);
      return next;
    });
  }

  function addJob() {
    let i = 1;
    while (model.jobs[`job_${i}`]) i++;
    const jobKey = `job_${i}`;
    const job: Job = {
      runs_on: "self-hosted",
      needs: [],
      steps: [{ name: "Run", run: "echo hello", "continue-on-error": false }],
      artifacts: [],
      download_artifacts: [],
    };
    const nextModel: WorkflowModel = { ...model, jobs: { ...model.jobs, [jobKey]: job } };
    setModel(nextModel);
    const g = toReactFlow(nextModel);
    setNodes(g.nodes);
    setEdges(g.edges);
    setSelectedId(jobKey);
  }

  function updateSelectedJob(job: Job) {
    if (!selectedId || selectedId === "__trigger__") return;
    const nextModel = { ...model, jobs: { ...model.jobs, [selectedId]: job } };
    setModel(nextModel);
    setNodes((nds) => nds.map((n) => (n.id === selectedId ? { ...n, data: { ...n.data, job } } : n)));
  }

  function removeSelectedJob() {
    if (!selectedId || selectedId === "__trigger__") return;
    const { [selectedId]: _removed, ...rest } = model.jobs;
    const nextModel = { ...model, jobs: rest };
    setModel(nextModel);
    const g = toReactFlow(nextModel);
    setNodes(g.nodes);
    setEdges(g.edges);
    setSelectedId(null);
  }

  async function save() {
    const result =
      mode === "code"
        ? await onSave({ yaml_source: yamlText })
        : await onSave({ workflow_json: fromReactFlow(name, nodes, edges) });
    setYamlText(result.yaml_source);
    const { model: parsed } = parseYaml(result.yaml_source, name);
    if (parsed) {
      setModel(parsed);
      const g = toReactFlow(parsed);
      setNodes(g.nodes);
      setEdges(g.edges);
    }
  }

  const selectedJob = selectedId && selectedId !== "__trigger__" ? model.jobs[selectedId] : null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-neutral-800 pb-3">
        <div className="flex gap-1 rounded-md border border-neutral-800 p-0.5">
          <button
            type="button"
            onClick={switchToVisual}
            className={`rounded px-3 py-1 text-xs font-medium ${mode === "visual" ? "bg-accent text-white" : "text-neutral-400 hover:text-neutral-200"}`}
          >
            Visual builder
          </button>
          <button
            type="button"
            onClick={switchToCode}
            className={`rounded px-3 py-1 text-xs font-medium ${mode === "code" ? "bg-accent text-white" : "text-neutral-400 hover:text-neutral-200"}`}
          >
            YAML
          </button>
        </div>
        <div className="flex items-center gap-3">
          {saveError && <span className="text-xs text-[var(--color-status-error)]">{saveError}</span>}
          <button
            type="button"
            onClick={save}
            disabled={saving}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-60"
          >
            <Save className="h-3.5 w-3.5" strokeWidth={2} />
            {saving ? "Saving…" : "Save workflow"}
          </button>
        </div>
      </div>

      <div className="mt-3 min-h-0 flex-1">
        {mode === "code" ? (
          <YamlCodeEditor value={yamlText} onChange={setYamlText} error={parseError} />
        ) : (
          <div className="flex h-full gap-3">
            <div className="min-w-0 flex-1 rounded-lg border border-neutral-800">
              <div className="border-b border-neutral-800 p-2">
                <button
                  type="button"
                  onClick={addJob}
                  className="inline-flex items-center gap-1 rounded-md border border-neutral-700 px-2.5 py-1 text-xs text-neutral-200 hover:bg-neutral-800"
                >
                  <Plus className="h-3 w-3" strokeWidth={2} />
                  Add job
                </button>
              </div>
              <ReactFlow
                nodes={nodes}
                edges={edges}
                nodeTypes={NODE_TYPES}
                onNodesChange={onNodesChange}
                onEdgesChange={onEdgesChange}
                onConnect={onConnect}
                onNodeClick={(_, node) => setSelectedId(node.id)}
                onPaneClick={() => setSelectedId(null)}
                fitView
                proOptions={{ hideAttribution: true }}
              >
                <Background gap={16} color={resolvedTheme === "dark" ? "#232631" : "#d8dae0"} />
                <Controls />
              </ReactFlow>
            </div>
            <div className="w-80 shrink-0 overflow-y-auto rounded-lg border border-neutral-800 bg-neutral-900 p-4">
              {selectedId === "__trigger__" && (
                <TriggerConfigPanel on={model.on} onChange={(on) => setModel((m) => ({ ...m, on }))} />
              )}
              {selectedJob && selectedId && (
                <JobConfigPanel jobKey={selectedId} job={selectedJob} onChange={updateSelectedJob} onRemove={removeSelectedJob} />
              )}
              {!selectedId && <p className="text-sm text-neutral-500">Select the trigger or a job to edit its configuration.</p>}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
