import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowRight, Plus } from "lucide-react";
import { useRepos } from "../hooks/useRepos";
import { useAnalyticsSummary, useDurationTrend, useStatusBreakdown } from "../hooks/useAnalytics";
import { useRuns } from "../hooks/useRuns";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import DurationTrendChart from "../components/analytics/DurationTrendChart";
import StatusBreakdownChart from "../components/analytics/StatusBreakdownChart";
import StatusBadge from "../components/common/StatusBadge";

export default function DashboardPage() {
  const { data: repos } = useRepos();
  const [repoId, setRepoId] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!repoId && repos && repos.length > 0) setRepoId(repos[0].id);
  }, [repos, repoId]);

  const { data: summary } = useAnalyticsSummary(repoId);
  const { data: trend } = useDurationTrend(repoId);
  const { data: statuses } = useStatusBreakdown(repoId);
  const { data: recentRuns } = useRuns(repoId, 8);

  if (repos && repos.length === 0) {
    return (
      <div>
        <h1 className="text-lg font-semibold text-neutral-100">Dashboard</h1>
        <p className="mt-4 text-sm text-neutral-500">No repos connected yet.</p>
        <Link
          to="/repos/connect"
          className="mt-3 inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
        >
          <Plus className="h-3.5 w-3.5" strokeWidth={2} />
          Connect a repo
        </Link>
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Dashboard</h1>
        <select
          value={repoId ?? ""}
          onChange={(e) => setRepoId(e.target.value)}
          className="rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1 text-sm text-neutral-200 outline-none focus:border-accent"
        >
          {(repos ?? []).map((r) => (
            <option key={r.id} value={r.id}>
              {r.owner}/{r.name}
            </option>
          ))}
        </select>
      </div>

      {summary && (
        <div className="mt-5">
          <SuccessRateChart summary={summary} />
        </div>
      )}

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
        {trend && <DurationTrendChart points={trend} />}
        {statuses && <StatusBreakdownChart counts={statuses} />}
      </div>

      <div className="mt-6">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-neutral-200">Recent runs</h2>
          {repoId && (
            <Link to={`/repos/${repoId}/runs`} className="inline-flex items-center gap-1 text-xs text-accent hover:underline">
              View all
              <ArrowRight className="h-3 w-3" strokeWidth={2} />
            </Link>
          )}
        </div>
        <div className="mt-2 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
          {(recentRuns ?? []).map((run) => (
            <Link key={run.id} to={`/runs/${run.id}`} className="flex items-center justify-between px-4 py-2.5 hover:bg-neutral-800/50">
              <span className="text-sm text-neutral-300">
                {run.trigger_event}
                {run.ref_name ? ` · ${run.ref_name}` : ""}
              </span>
              <StatusBadge status={run.status} />
            </Link>
          ))}
          {(recentRuns ?? []).length === 0 && <div className="px-4 py-5 text-sm text-neutral-500">No runs yet.</div>}
        </div>
      </div>
    </div>
  );
}
