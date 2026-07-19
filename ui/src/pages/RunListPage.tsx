import { Link, useParams } from "react-router-dom";
import { Clock, PlayCircle } from "lucide-react";
import { useRuns } from "../hooks/useRuns";
import StatusBadge from "../components/common/StatusBadge";
import PageHeader from "../components/common/PageHeader";
import { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

export default function RunListPage() {
  const { repoId } = useParams();
  const { data: runs, isLoading } = useRuns(repoId, 100);

  return (
    <div>
      <PageHeader title="Runs" />

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass("mt-4")}>
        {(runs ?? []).map((run) => (
          <Link key={run.id} to={`/runs/${run.id}`} className="flex items-center justify-between px-4 py-3 hover:bg-neutral-800/50">
            <div>
              <div className="text-sm text-neutral-200">
                {run.trigger_event}
                {run.ref_name ? (
                  <>
                    {" · "}
                    <span className="font-mono">{run.ref_name}</span>
                  </>
                ) : (
                  ""
                )}
              </div>
              <div className="mt-0.5 flex items-center gap-1 text-xs text-neutral-500">
                <Clock className="h-3 w-3" strokeWidth={2} />
                {new Date(run.created_at).toLocaleString()}
              </div>
            </div>
            <StatusBadge status={run.status} />
          </Link>
        ))}
        {(runs ?? []).length === 0 && !isLoading && <EmptyState icon={PlayCircle} message="No runs yet." />}
      </div>
    </div>
  );
}
