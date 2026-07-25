import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, FolderGit2 } from "lucide-react";
import { useRepos } from "../hooks/useRepos";
import { useAnalyticsSummary } from "../hooks/useAnalytics";
import SuccessRateChart from "../components/analytics/SuccessRateChart";
import DashboardSidebar from "../components/layout/DashboardSidebar";
import Select from "../components/common/Select";
import PageHeader from "../components/common/PageHeader";
import Card from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

export default function DashboardPage() {
  const { data: repos, isLoading } = useRepos();
  const [repoId, setRepoId] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!repoId && repos && repos.length > 0) setRepoId(repos[0].id);
  }, [repos, repoId]);

  const { data: summary } = useAnalyticsSummary(repoId);

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-[260px_1fr]">
      <DashboardSidebar />

      <div className="min-w-0">
        <PageHeader title="Dashboard" />

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
      </div>
    </div>
  );
}
