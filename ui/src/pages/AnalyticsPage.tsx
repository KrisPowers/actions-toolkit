import { Link, useParams } from "react-router-dom";
import { ArrowLeft, ArrowRight } from "lucide-react";
import { useRepo } from "../hooks/useRepos";
import { useAnalyticsSummary, useDurationTrend, useStatusBreakdown } from "../hooks/useAnalytics";
import { useRuns } from "../hooks/useRuns";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import DurationTrendChart from "../components/analytics/DurationTrendChart";
import StatusBreakdownChart from "../components/analytics/StatusBreakdownChart";
import StatusBadge from "../components/common/StatusBadge";

export default function AnalyticsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: summary } = useAnalyticsSummary(repoId);
  const { data: trend } = useDurationTrend(repoId);
  const { data: statuses } = useStatusBreakdown(repoId);
  const { data: recentRuns } = useRuns(repoId, 20);

  return (
    <div>
      <Link to="/" className="inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300">
        <ArrowLeft className="h-3 w-3" strokeWidth={2} />
        Dashboard
      </Link>
      <h1 className="mt-0.5 text-lg font-semibold text-neutral-100">
        Analytics{repo ? ` · ${repo.owner}/${repo.name}` : ""}
      </h1>

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
