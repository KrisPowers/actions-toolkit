import { Link, useParams } from "react-router-dom";
import { ArrowLeft, Download, Package } from "lucide-react";
import { useArtifacts } from "../hooks/useArtifacts";
import { artifactsApi } from "../api/artifacts";
import { buttonClass } from "../components/common/Button";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function ArtifactsPage() {
  const { runId } = useParams();
  const { data: artifacts, isLoading } = useArtifacts(runId);

  return (
    <div>
      <Link to={`/runs/${runId}`} className="inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300">
        <ArrowLeft className="h-3 w-3" strokeWidth={2} />
        Back to run
      </Link>
      <h1 className="mt-1 text-lg font-semibold text-neutral-100">Artifacts</h1>

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
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
        {(artifacts ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No artifacts produced by this run.</div>}
      </div>
    </div>
  );
}
