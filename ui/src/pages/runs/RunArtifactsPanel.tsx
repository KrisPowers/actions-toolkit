import { useParams } from "react-router-dom";
import { Download, Package } from "lucide-react";
import { useArtifacts } from "../../hooks/useArtifacts";
import { artifactsApi } from "../../api/artifacts";
import { buttonClass } from "../../components/common/Button";
import { listCardClass } from "../../components/common/Card";
import EmptyState from "../../components/common/EmptyState";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function RunArtifactsPanel() {
  const { runId } = useParams();
  const { data: artifacts, isLoading } = useArtifacts(runId);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      {isLoading && <p className="text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass()}>
        {(artifacts ?? []).map((a) => (
          <div key={a.id} className="flex items-center justify-between px-4 py-3">
            <div className="flex items-center gap-2">
              <Package className="h-4 w-4 text-neutral-500" strokeWidth={2} />
              <div>
                <div className="text-sm text-neutral-200">{a.name}</div>
                <div className="mt-0.5 text-xs text-neutral-500">{formatBytes(a.size_bytes)}</div>
              </div>
            </div>
            <a href={artifactsApi.downloadUrl(a.id)} className={buttonClass("default", "sm")}>
              <Download className="h-3.5 w-3.5" strokeWidth={2} />
              Download
            </a>
          </div>
        ))}
        {(artifacts ?? []).length === 0 && !isLoading && <EmptyState icon={Package} message="No artifacts produced by this run." />}
      </div>
    </div>
  );
}
