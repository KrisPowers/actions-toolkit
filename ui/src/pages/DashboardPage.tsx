import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, FolderGit2, GitBranch, Plus } from "lucide-react";
import { useRepos } from "../hooks/useRepos";
import { useAnalyticsSummary } from "../hooks/useAnalytics";
import SuccessRateChart from "../components/analytics/SuccessRateChart";

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
        <Link
          to="/repos/connect"
          className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
        >
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
            {repoId && (
              <Link
                to={`/analytics/${repoId}`}
                className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
              >
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
            <Link
              key={r.id}
              to={`/repos/${r.id}/workflows`}
              className="rounded-lg border border-neutral-800 bg-neutral-900 p-4 hover:border-accent/50"
            >
              <div className="flex items-center gap-2 text-sm font-semibold text-neutral-100">
                <FolderGit2 className="h-4 w-4 shrink-0 text-neutral-500" strokeWidth={2} />
                {r.owner}/{r.name}
              </div>
              <div className="mt-1.5 flex items-center gap-1 text-xs text-neutral-500">
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
