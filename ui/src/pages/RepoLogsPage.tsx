import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ArrowUpRight } from "lucide-react";
import { useRuns } from "../hooks/useRuns";
import { useLiveLogs } from "../hooks/useLiveLogs";
import StatusBadge from "../components/common/StatusBadge";
import LogViewer from "../components/logs/LogViewer";
import Select from "../components/common/Select";

const ACTIVE_STATUSES = new Set(["queued", "running", "pending"]);

export default function RepoLogsPage() {
  const { repoId } = useParams();
  const { data: runs } = useRuns(repoId, 20);
  const [runId, setRunId] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!runId && runs && runs.length > 0) setRunId(runs[0].id);
  }, [runs, runId]);

  const selectedRun = runs?.find((r) => r.id === runId);
  const runActive = selectedRun ? ACTIVE_STATUSES.has(selectedRun.status) : false;
  const lines = useLiveLogs(runId, runActive);

  return (
    <div className="flex h-[calc(100vh-6.5rem)] flex-col">
      <div className="flex items-center justify-between pb-3">
        <div>
          <h1 className="text-lg font-semibold text-neutral-100">Logs</h1>
          <p className="mt-0.5 text-sm text-neutral-400">Live console for a run, without leaving the repo.</p>
        </div>
        <div className="flex items-center gap-3">
          <Select value={runId ?? ""} onChange={(e) => setRunId(e.target.value)}>
            {(runs ?? []).map((r) => (
              <option key={r.id} value={r.id}>
                {r.trigger_event}
                {r.ref_name ? ` · ${r.ref_name}` : ""} · {new Date(r.created_at).toLocaleString()}
              </option>
            ))}
          </Select>
          {selectedRun && <StatusBadge status={selectedRun.status} />}
          {runId && (
            <Link
              to={`/runs/${runId}`}
              className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
            >
              Full run
              <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={2} />
            </Link>
          )}
        </div>
      </div>

      {(runs ?? []).length === 0 ? (
        <p className="text-sm text-neutral-500">No runs yet, so there's nothing to tail.</p>
      ) : (
        <div className="min-h-0 flex-1">
          <LogViewer lines={lines} />
        </div>
      )}
    </div>
  );
}
