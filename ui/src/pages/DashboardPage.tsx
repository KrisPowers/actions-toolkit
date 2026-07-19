import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, GitBranch, Plus } from "lucide-react";
import { useRepos } from "../hooks/useRepos";
import { useAnalyticsSummary } from "../hooks/useAnalytics";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import Avatar from "../components/common/Avatar";
import Select from "../components/common/Select";
import { buttonClass } from "../components/common/Button";

export default function DashboardPage() {
  const { data: repos, isLoading } = useRepos();
  const [repoId, setRepoId] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!repoId && repos && repos.length > 0) setRepoId(repos[0].id);
  }, [repos, repoId]);

  const { data: summary } = useAnalyticsSummary(repoId);

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Dashboard</h1>
        <Link to="/repos/connect" className={buttonClass("primary")}>
          <Plus className="h-3.5 w-3.5" strokeWidth={2} />
          Connect a repo
        </Link>
      </div>

      {!isLoading && (repos ?? []).length === 0 && (
        <p className="mt-6 text-sm text-neutral-500">No repos connected yet. Connect one to start running workflows locally.</p>
      )}

      {(repos ?? []).length > 0 && (
        <div className="mt-5 rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <div className="flex items-center justify-between gap-3">
            <Select value={repoId ?? ""} onChange={(e) => setRepoId(e.target.value)} className="py-1">
              {(repos ?? []).map((r) => (
                <option key={r.id} value={r.id}>
                  {r.owner}/{r.name}
                </option>
              ))}
            </Select>
            {repoId && (
              <Link to={`/analytics/${repoId}`} className="inline-flex items-center gap-1 text-xs text-accent hover:underline">
                Full analytics
                <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={2} />
              </Link>
            )}
          </div>
          {summary && (
            <div className="mt-3">
              <SuccessRateChart summary={summary} />
            </div>
          )}
        </div>
      )}

      <div className="mt-6">
        <h2 className="text-sm font-semibold text-neutral-200">Repositories</h2>
        <div className="mt-2 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {(repos ?? []).map((r) => (
            <Link key={r.id} to={`/repos/${r.id}/workflows`} className="rounded-lg border border-neutral-800 bg-neutral-900 p-4 hover:border-accent/50">
              <div className="flex items-center gap-2 text-sm font-semibold text-neutral-100">
                <Avatar login={r.owner} size={18} />
                {r.owner}/{r.name}
              </div>
              <div className="mt-1.5 flex items-center gap-1 font-mono text-xs text-neutral-500">
                <GitBranch className="h-3 w-3" strokeWidth={2} />
                {r.default_branch}
              </div>
            </Link>
          ))}
        </div>
      </div>
    </div>
  );
}
