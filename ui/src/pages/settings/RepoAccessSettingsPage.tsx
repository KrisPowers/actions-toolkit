import { Link, useParams } from "react-router-dom";
import { RefreshCw } from "lucide-react";
import { useRepo, useTestRepoConnection } from "../../hooks/useRepos";
import GithubMark from "../../components/common/GithubMark";
import Button from "../../components/common/Button";
import Card from "../../components/common/Card";

export default function RepoAccessSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const testConnection = useTestRepoConnection();

  if (!repo) return null;

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <GithubMark className="h-4 w-4 text-neutral-500" />
        <div className="text-sm font-medium text-neutral-200">GitHub access</div>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        Uses the GitHub connection in{" "}
        <Link to="/settings" className="text-accent hover:underline">
          Settings
        </Link>
        .
      </p>

      <div className="mt-4 border-t border-neutral-800 pt-4">
        <Button variant="default" onClick={() => testConnection.mutate(repo.id)} disabled={testConnection.isPending}>
          <RefreshCw className={`h-3.5 w-3.5 ${testConnection.isPending ? "animate-spin" : ""}`} strokeWidth={2} />
          {testConnection.isPending ? "Testing…" : "Test connection"}
        </Button>
        {testConnection.data && (
          <p className={`mt-2 text-sm ${testConnection.data.ok ? "text-[var(--color-status-success)]" : "text-[var(--color-status-error)]"}`}>
            {testConnection.data.message}
          </p>
        )}
      </div>
    </Card>
  );
}
