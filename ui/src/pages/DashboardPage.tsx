import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, FolderGit2, GitBranch, Plus } from "lucide-react";
import { useRepos } from "../hooks/useRepos";
import { useAnalyticsSummary } from "../hooks/useAnalytics";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import Avatar from "../components/common/Avatar";
import Select from "../components/common/Select";
import { buttonClass } from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import Card, { cardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

export default function DashboardPage() {
  const { data: repos, isLoading } = useRepos();
  const [repoId, setRepoId] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!repoId && repos && repos.length > 0) setRepoId(repos[0].id);
  }, [repos, repoId]);

  const { data: summary } = useAnalyticsSummary(repoId);

  return (
    <div>
      <PageHeader
        title="Dashboard"
        actions={
          <Link to="/repos/connect" className={buttonClass("primary")}>
            <Plus className="h-3.5 w-3.5" strokeWidth={2} />
            Connect a repo
          </Link>
        }
      />

      {!isLoading && (repos ?? []).length === 0 && (
        <Card className="mt-5">
          <EmptyState icon={FolderGit2} message="No repos connected yet. Connect one to start running workflows locally." />
        </Card>
      )}

      {(repos ?? []).length > 0 && (
        <Card className="mt-5 p-4">
          <div className="flex items-center justify-between gap-3">
            <Select value={repoId ?? ""} onChange={(e) => setRepoId(e.target.value)}>
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
        </Card>
      )}

      <div className="mt-6">
        <h2 className="text-sm font-semibold text-neutral-200">Repositories</h2>
        <div className="mt-2 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {(repos ?? []).map((r) => (
            <Link key={r.id} to={`/repos/${r.id}/overview`} className={cardClass("p-4 transition-colors hover:border-accent/50")}>
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
