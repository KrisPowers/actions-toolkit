import { Link, useParams } from "react-router-dom";
import { useRuns } from "../hooks/useRuns";
import StatusBadge from "../components/common/StatusBadge";

export default function RunListPage() {
  const { repoId } = useParams();
  const { data: runs, isLoading } = useRuns(repoId, 100);

  return (
    <div>
      <h1 className="text-lg font-semibold text-neutral-100">Runs</h1>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(runs ?? []).map((run) => (
          <Link key={run.id} to={`/runs/${run.id}`} className="flex items-center justify-between px-4 py-3 hover:bg-neutral-800/50">
            <div>
              <div className="text-sm text-neutral-200">
                {run.trigger_event}
                {run.ref_name ? ` · ${run.ref_name}` : ""}
              </div>
              <div className="mt-0.5 text-xs text-neutral-500">{new Date(run.created_at).toLocaleString()}</div>
            </div>
            <StatusBadge status={run.status} />
          </Link>
        ))}
        {(runs ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No runs yet.</div>}
      </div>
    </div>
  );
}
