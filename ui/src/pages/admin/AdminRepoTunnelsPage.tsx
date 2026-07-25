import { Link } from "react-router-dom";
import { AlertTriangle, CheckCircle2, CircleDashed, Loader2, Radio } from "lucide-react";
import { useRepos } from "../../hooks/useRepos";
import { useRepoTunnelStatus } from "../../hooks/useSettings";
import Card from "../../components/common/Card";
import type { RepoPublic } from "../../api/types";

function TunnelStatusIndicator({ repoId }: { repoId: string }) {
  const { data: status } = useRepoTunnelStatus(repoId);

  if (!status || status.status === "idle") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-neutral-500">
        <CircleDashed className="h-3.5 w-3.5" strokeWidth={2} />
        No tunnel running
      </span>
    );
  }

  if (status.status === "starting") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-[var(--color-status-warning)]">
        <Loader2 className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
        Starting…
      </span>
    );
  }

  if (status.status === "failed") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-[var(--color-status-error)]" title={status.message}>
        <AlertTriangle className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
        Failed to start
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-[var(--color-status-success)]">
      <CheckCircle2 className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
      <code className="text-neutral-400">{status.url}</code>
    </span>
  );
}

function RepoTunnelRow({ repo }: { repo: RepoPublic }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-neutral-800 px-3 py-2 text-sm">
      <div className="min-w-0">
        <div className="truncate text-neutral-200">
          {repo.owner}/{repo.name}
        </div>
        <div className="mt-0.5">
          <TunnelStatusIndicator repoId={repo.id} />
        </div>
      </div>
      <Link to={`/repos/${repo.id}/webhooks`} className="shrink-0 text-xs text-accent hover:underline">
        Manage
      </Link>
    </div>
  );
}

/**
 * Read-only rollup of every connected repo's webhook tunnel, each of which is otherwise only
 * visible one at a time on that repo's own Webhooks page. Nothing here starts or stops a
 * tunnel -- that stays on the repo's Webhooks page, this is just visibility across all of them
 * at once.
 */
export default function AdminRepoTunnelsPage() {
  const { data: repos } = useRepos();

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <Radio className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <h2 className="text-sm font-semibold text-neutral-200">Repo tunnels</h2>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        Every connected repo's webhook tunnel, independent of this instance's own Remote access tunnel.
      </p>

      <div className="mt-3 flex flex-col gap-2">
        {(repos ?? []).map((repo) => (
          <RepoTunnelRow key={repo.id} repo={repo} />
        ))}
        {repos?.length === 0 && <p className="text-sm text-neutral-500">No repos connected yet.</p>}
      </div>
    </Card>
  );
}
