import { Link, useParams } from "react-router-dom";
import { Package } from "lucide-react";
import { useRepoArtifacts } from "../hooks/useArtifacts";
import { artifactsApi } from "../api/artifacts";
import StatusBadge from "../components/common/StatusBadge";
import { buttonClass } from "../components/common/Button";
import PageHeader from "../components/common/PageHeader";
import { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function RepoArtifactsPage() {
  const { repoId } = useParams();
  const { data: artifacts, isLoading } = useRepoArtifacts(repoId);

  return (
    <div>
      <PageHeader title="Artifacts" subtitle="Everything produced by every run in this repo, newest first." />

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass("mt-4")}>
        {(artifacts ?? []).map((a) => (
          <div key={a.id} className="flex items-center justify-between gap-3 px-4 py-3">
            <div className="flex min-w-0 items-center gap-2">
              <Package className="h-4 w-4 shrink-0 text-neutral-500" strokeWidth={2} />
              <div className="min-w-0">
                <div className="truncate text-sm text-neutral-200">{a.name}</div>
                <div className="mt-0.5 flex items-center gap-2 text-xs text-neutral-500">
                  <span>{formatBytes(a.size_bytes)}</span>
                  <span>·</span>
                  <Link to={`/runs/${a.workflow_run_id}`} className="hover:text-accent hover:underline">
                    {a.workflow_name}
                  </Link>
                  <span>·</span>
                  <span>{new Date(a.created_at).toLocaleString()}</span>
                </div>
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-3">
              <StatusBadge status={a.run_status} />
              <a href={artifactsApi.downloadUrl(a.id)} className={buttonClass("default", "sm")}>
                Download
              </a>
            </div>
          </div>
        ))}
        {(artifacts ?? []).length === 0 && !isLoading && <EmptyState icon={Package} message="No artifacts produced by this repo yet." />}
      </div>
    </div>
  );
}
