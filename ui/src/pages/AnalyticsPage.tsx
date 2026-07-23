import { Link, useParams } from "react-router-dom";
import { ArrowRight, PlayCircle } from "lucide-react";
import { useRepo } from "../hooks/useRepos";
import { useAnalyticsSummary, useDurationTrend, useStatusBreakdown } from "../hooks/useAnalytics";
import { useRuns } from "../hooks/useRuns";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import DurationTrendChart from "../components/analytics/DurationTrendChart";
import StatusBreakdownChart from "../components/analytics/StatusBreakdownChart";
import StatusBadge from "../components/common/StatusBadge";
import PageHeader from "../components/common/PageHeader";
import { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

export default function AnalyticsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: summary } = useAnalyticsSummary(repoId);
  const { data: trend } = useDurationTrend(repoId);
  const { data: statuses } = useStatusBreakdown(repoId);
  const { data: recentRuns } = useRuns(repoId, 20);

  return (
    <div>
      <PageHeader title={`Analytics${repo ? ` · ${repo.owner}/${repo.name}` : ""}`} backTo="/" backLabel="Dashboard" />

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
            <Link to={`/repos/${repoId}/overview`} className="inline-flex items-center gap-1 text-xs text-accent hover:underline">
              View all
              <ArrowRight className="h-3 w-3" strokeWidth={2} />
            </Link>
          )}
        </div>
        <div className={listCardClass("mt-2")}>
          {(recentRuns ?? []).map((run) => (
            <Link key={run.id} to={`/runs/${run.id}`} className="flex items-center justify-between px-4 py-2.5 hover:bg-neutral-800/50">
              <span className="text-sm text-neutral-300">
                {run.trigger_event}
                {run.ref_name ? ` · ${run.ref_name}` : ""}
              </span>
              <StatusBadge status={run.status} />
            </Link>
          ))}
          {(recentRuns ?? []).length === 0 && <EmptyState icon={PlayCircle} message="No runs yet." />}
        </div>
      </div>
    </div>
  );
}
